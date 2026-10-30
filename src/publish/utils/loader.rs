use crate::shared::credentials::{effective_s3, ResolvedS3};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::{Config, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use std::fs;
use std::io::Read;
use std::time::Duration;

const MAX_RETRIES: u32 = 3;

fn s3_settings() -> ResolvedS3 {
    effective_s3()
}
const MULTIPART_THRESHOLD: u64 = 100 * 1024 * 1024;
const PART_SIZE: usize = 16 * 1024 * 1024;

/// 上传文件到又拍云（流式读取，适合大模型文件）
pub fn upload_file(file_path: &str, bucket_name: &str, object_name: &str) -> bool {
    let size = fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);
    println!("  文件大小: {} bytes", size);

    for attempt in 1..=MAX_RETRIES {
        println!(
            "   上传: {} (尝试 {}/{})",
            object_name, attempt, MAX_RETRIES
        );
        if upload_path_to_s3(file_path, bucket_name, object_name) {
            println!("   文件上传成功: {}", object_name);
            return true;
        }
        if attempt < MAX_RETRIES {
            println!("  上传失败，{}秒后重试...", attempt);
            std::thread::sleep(Duration::from_secs(attempt as u64));
        }
    }

    eprintln!("  文件上传失败: 已达到最大重试次数 ({})", object_name);
    false
}

/// 流式上传路径，避免把整个大文件读入内存
fn upload_path_to_s3(file_path: &str, bucket_name: &str, object_name: &str) -> bool {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  创建运行时失败: {}", e);
            return false;
        }
    };

    rt.block_on(async {
        let credentials = Credentials::new(
            s3_settings().access_key.unwrap_or_default(),
            s3_settings().secret_key.unwrap_or_default(),
            None,
            None,
            "custom",
        );
        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(s3_settings().endpoint)
            .region(Region::new("us-east-1"))
            .credentials_provider(credentials)
            .load()
            .await;
        let config = Config::from(&config)
            .to_builder()
            .force_path_style(true)
            .build();
        let client = Client::from_conf(config);

        let size = fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);
        if size > MULTIPART_THRESHOLD {
            return match upload_multipart(&client, file_path, bucket_name, object_name).await {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("  S3分片上传失败: {}", e);
                    false
                }
            };
        }

        let body = match ByteStream::from_path(file_path).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  S3上传失败（读取文件）: {}", e);
                return false;
            }
        };

        match client
            .put_object()
            .bucket(bucket_name)
            .key(object_name)
            .body(body)
            .acl(aws_sdk_s3::types::ObjectCannedAcl::PublicRead)
            .send()
            .await
        {
            Ok(_) => true,
            Err(e) => {
                eprintln!("  S3上传失败: {}", e);
                false
            }
        }
    })
}

/// 上传内存内容到又拍云（带重试机制）
pub fn upload_bytes(content: &[u8], bucket_name: &str, object_name: &str) -> bool {
    for attempt in 1..=MAX_RETRIES {
        println!(
            "   上传: {} (尝试 {}/{})",
            object_name, attempt, MAX_RETRIES
        );

        if upload_to_s3(content, bucket_name, object_name) {
            println!("   文件上传成功: {}", object_name);
            return true;
        }

        if attempt < MAX_RETRIES {
            println!("  上传失败，{}秒后重试...", attempt);
            std::thread::sleep(Duration::from_secs(attempt as u64));
        }
    }

    eprintln!("  文件上传失败: 已达到最大重试次数 ({})", object_name);
    false
}

/// 判断 S3 对象是否存在（HEAD）。404 / NoSuchKey 返回 false，其它错误返回 Err。
pub fn object_exists(bucket_name: &str, object_name: &str) -> Result<bool, String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("创建运行时失败: {}", e))?;

    rt.block_on(async {
        let credentials = Credentials::new(
            s3_settings().access_key.unwrap_or_default(),
            s3_settings().secret_key.unwrap_or_default(),
            None,
            None,
            "custom",
        );

        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(s3_settings().endpoint)
            .region(Region::new("us-east-1"))
            .credentials_provider(credentials)
            .load()
            .await;

        let config = Config::from(&config)
            .to_builder()
            .force_path_style(true)
            .build();

        let client = Client::from_conf(config);

        match client
            .head_object()
            .bucket(bucket_name)
            .key(object_name)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let text = format!("{} {:?}", e, e);
                let lower = text.to_lowercase();
                if text.contains("404")
                    || text.contains("NotFound")
                    || text.contains("NoSuchKey")
                    || lower.contains("not found")
                {
                    Ok(false)
                } else {
                    Err(format!("HEAD {} 失败: {}", object_name, text))
                }
            }
        }
    })
}

/// 列出 S3 指定前缀下的对象 key。
///
/// 又拍云对不存在的 key 的 HEAD/GET 可能返回 403，发布索引时用 ListObjectsV2
/// 判断 `packages.json` 是否真实存在，避免误覆盖已有索引。
pub fn list_keys(bucket_name: &str, prefix: &str) -> Result<Vec<String>, String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("创建运行时失败: {}", e))?;

    rt.block_on(async {
        let credentials = Credentials::new(
            s3_settings().access_key.unwrap_or_default(),
            s3_settings().secret_key.unwrap_or_default(),
            None,
            None,
            "custom",
        );

        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(s3_settings().endpoint)
            .region(Region::new("us-east-1"))
            .credentials_provider(credentials)
            .load()
            .await;

        let config = Config::from(&config)
            .to_builder()
            .force_path_style(true)
            .build();

        let client = Client::from_conf(config);
        let mut keys = Vec::new();
        let mut continuation: Option<String> = None;

        loop {
            let mut request = client.list_objects_v2().bucket(bucket_name).prefix(prefix);
            if let Some(token) = &continuation {
                request = request.continuation_token(token.clone());
            }

            let response = request
                .send()
                .await
                .map_err(|e| format!("ListObjectsV2 {} 失败: {}", prefix, e))?;

            for object in response.contents() {
                if let Some(key) = object.key() {
                    keys.push(key.to_string());
                }
            }

            if response.is_truncated().unwrap_or(false) {
                continuation = response
                    .next_continuation_token()
                    .map(|token| token.to_string());
                if continuation.is_none() {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(keys)
    })
}

/// 从 S3 下载文件内容
pub fn download_from_s3(bucket_name: &str, object_name: &str) -> Result<String, String> {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            return Err(format!("创建运行时失败: {}", e));
        }
    };

    rt.block_on(async {
        let credentials = Credentials::new(
            s3_settings().access_key.unwrap_or_default(),
            s3_settings().secret_key.unwrap_or_default(),
            None,
            None,
            "custom",
        );

        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(s3_settings().endpoint)
            .region(Region::new("us-east-1"))
            .credentials_provider(credentials)
            .load()
            .await;

        let config = Config::from(&config)
            .to_builder()
            .force_path_style(true)
            .build();

        let client = Client::from_conf(config);

        let result = client
            .get_object()
            .bucket(bucket_name)
            .key(object_name)
            .send()
            .await;

        match result {
            Ok(response) => {
                let bytes = response.body.collect().await;
                match bytes {
                    Ok(data) => match String::from_utf8(data.to_vec()) {
                        Ok(text) => Ok(text),
                        Err(e) => Err(format!("UTF-8 转换失败: {}", e)),
                    },
                    Err(e) => Err(format!("读取数据失败: {}", e)),
                }
            }
            Err(e) => Err(format!("下载失败: {}", e)),
        }
    })
}

/// 执行 S3 上传
fn upload_to_s3(file_content: &[u8], bucket_name: &str, object_name: &str) -> bool {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  创建运行时失败: {}", e);
            return false;
        }
    };

    rt.block_on(async {
        let credentials = Credentials::new(
            s3_settings().access_key.unwrap_or_default(),
            s3_settings().secret_key.unwrap_or_default(),
            None,
            None,
            "custom",
        );

        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(s3_settings().endpoint)
            .region(Region::new("us-east-1"))
            .credentials_provider(credentials)
            .load()
            .await;

        let config = Config::from(&config)
            .to_builder()
            .force_path_style(true)
            .build();

        let client = Client::from_conf(config);

        let result = client
            .put_object()
            .bucket(bucket_name)
            .key(object_name)
            .body(ByteStream::from(file_content.to_vec()))
            .acl(aws_sdk_s3::types::ObjectCannedAcl::PublicRead)
            .send()
            .await;

        match result {
            Ok(_) => true,
            Err(e) => {
                eprintln!("  S3上传失败: {}", e);
                false
            }
        }
    })
}

async fn upload_multipart(
    client: &Client,
    file_path: &str,
    bucket: &str,
    key: &str,
) -> Result<(), String> {
    use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};

    let created = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(key)
        .acl(aws_sdk_s3::types::ObjectCannedAcl::PublicRead)
        .send()
        .await
        .map_err(|e| format!("create_multipart_upload: {}", e))?;
    let upload_id = created
        .upload_id()
        .ok_or_else(|| "create_multipart_upload 未返回 upload_id".to_string())?
        .to_string();

    let result = async {
        let mut file = fs::File::open(file_path).map_err(|e| format!("打开文件失败: {}", e))?;
        let mut part_number = 1i32;
        let mut parts = Vec::new();
        loop {
            let mut buf = vec![0u8; PART_SIZE];
            let n = file
                .read(&mut buf)
                .map_err(|e| format!("读取文件失败: {}", e))?;
            if n == 0 {
                break;
            }
            buf.truncate(n);
            let response = client
                .upload_part()
                .bucket(bucket)
                .key(key)
                .upload_id(&upload_id)
                .part_number(part_number)
                .body(ByteStream::from(buf))
                .send()
                .await
                .map_err(|e| format!("upload_part {}: {}", part_number, e))?;
            parts.push(
                CompletedPart::builder()
                    .part_number(part_number)
                    .e_tag(response.e_tag().unwrap_or_default())
                    .build(),
            );
            println!(
                "    分片 {}/{} 完成",
                part_number,
                fs::metadata(file_path)
                    .map(|m| m.len())
                    .unwrap_or(0)
                    .div_ceil(PART_SIZE as u64)
            );
            part_number += 1;
        }

        client
            .complete_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(&upload_id)
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .set_parts(Some(parts))
                    .build(),
            )
            .send()
            .await
            .map_err(|e| format!("complete_multipart_upload: {}", e))?;
        Ok::<(), String>(())
    }
    .await;

    if result.is_err() {
        let _ = client
            .abort_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(&upload_id)
            .send()
            .await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::object_exists;

    #[test]
    #[ignore]
    fn s3_object_exists_live() {
        assert!(object_exists("packages", "design_plugins/packages.json").unwrap());
        assert!(!object_exists("packages", "design_plugins/__missing__/packages.json").unwrap());
    }
}
