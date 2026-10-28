use crate::shared::credentials::{effective_s3, ResolvedS3};
use anyhow::{anyhow, Context, Result};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::{Config, Region};
use aws_sdk_s3::Client;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_RETRIES: u32 = 3;

fn s3_settings() -> ResolvedS3 {
    effective_s3()
}

fn bucket() -> String {
    s3_settings().bucket
}

/// 创建访问又拍云 S3 的客户端
async fn create_client() -> Client {
    let credentials = Credentials::new(
        s3_settings().access_key.unwrap_or_default(),
        s3_settings().secret_key.unwrap_or_default(),
        None,
        None,
        "custom",
    );

    let shared = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(s3_settings().endpoint)
        .region(Region::new("us-east-1"))
        .credentials_provider(credentials)
        .load()
        .await;

    let config = Config::from(&shared)
        .to_builder()
        .force_path_style(true)
        .build();

    Client::from_conf(config)
}

/// 读取 S3 上的文本对象（用于 dytes.json / packages.json 这类小文件）
pub async fn fetch_text(key: &str) -> Result<String> {
    let mut last_error = None;

    for attempt in 1..=MAX_RETRIES {
        let client = create_client().await;
        match client.get_object().bucket(bucket()).key(key).send().await {
            Ok(response) => {
                let body = response
                    .body
                    .collect()
                    .await
                    .context("读取 S3 响应内容失败")?
                    .into_bytes();
                return String::from_utf8(body.to_vec())
                    .with_context(|| format!("{} 不是有效的 UTF-8 文本", key));
            }
            Err(e) => {
                last_error = Some(format!("{}", e));
                if attempt < MAX_RETRIES {
                    tokio::time::sleep(Duration::from_secs(attempt as u64)).await;
                }
            }
        }
    }

    Err(anyhow!(
        "读取 S3 对象 {} 失败: {}",
        key,
        last_error.unwrap_or_else(|| "未知错误".to_string())
    ))
}

/// 下载 S3 对象到本地文件。
///
/// 先写入 `.part` 临时文件，下载完成且长度校验通过后再重命名，
/// 避免下载中断后留下一个看起来完整的半截文件。
pub async fn download_to<F>(key: &str, dest: &Path, mut on_progress: F) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let client = create_client().await;

    let response = client
        .get_object()
        .bucket(bucket())
        .key(key)
        .send()
        .await
        .with_context(|| format!("下载 S3 对象 {} 失败", key))?;

    let total_size = response.content_length().unwrap_or(0) as u64;

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }

    let filename = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    let temp_path = dest.with_file_name(format!("{}.part", filename));

    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .with_context(|| format!("创建临时文件失败: {}", temp_path.display()))?;

    let mut stream = response.body.into_async_read();
    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 8192];

    loop {
        let n = stream
            .read(&mut buffer)
            .await
            .context("读取 S3 数据流失败")?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])
            .await
            .context("写入临时文件失败")?;
        downloaded += n as u64;
        on_progress(downloaded, total_size);
    }

    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    if total_size > 0 && downloaded != total_size {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(anyhow!(
            "下载不完整: 预期 {} 字节，实际 {} 字节",
            total_size,
            downloaded
        ));
    }

    // 某些对象在响应头里没有长度，此时至少要求非空
    if downloaded == 0 {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(anyhow!("下载到的文件为空: {}", key));
    }

    tokio::fs::rename(&temp_path, dest)
        .await
        .with_context(|| format!("重命名临时文件失败: {}", temp_path.display()))?;

    Ok(())
}
