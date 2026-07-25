use std::fs::File;
use std::io::Read;
use std::time::Duration;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Region, Config};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;

const ENDPOINT_URL: &str = "https://s3.api.upyun.com";
const ACCESS_KEY: &str = "195cc6d12c2d45389a5e52e5138f7250";
const SECRET_KEY: &str = "7ab145ae97310ca9f65db3dff07beac2";
const MAX_RETRIES: u32 = 3;

/// 上传文件到又拍云（带重试机制）
pub fn upload_file(file_path: &str, bucket_name: &str, object_name: &str) -> bool {
    for attempt in 1..=MAX_RETRIES {
        println!("   上传文件: {} (尝试 {}/{})", file_path, attempt, MAX_RETRIES);
        println!("   目标: {}/{}", bucket_name, object_name);
        
        // 读取文件内容
        let file_content = match read_file_content(file_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("  读取文件失败: {}", e);
                return false;
            }
        };
        
        println!("  文件大小: {} bytes", file_content.len());
        
        // 执行上传
        let success = upload_to_s3(&file_content, bucket_name, object_name);
        
        if success {
            println!("  文件上传成功: {}", object_name);
            return true;
        }
        
        if attempt < MAX_RETRIES {
            println!("  上传失败，{}秒后重试...", attempt);
            std::thread::sleep(Duration::from_secs(attempt as u64));
        }
    }
    
    eprintln!("  文件上传失败: 已达到最大重试次数");
    false
}

/// 读取文件内容
fn read_file_content(file_path: &str) -> Result<Vec<u8>, std::io::Error> {
    let mut file = File::open(file_path)?;
    let mut content = Vec::new();
    file.read_to_end(&mut content)?;
    Ok(content)
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
        let credentials = Credentials::new(ACCESS_KEY, SECRET_KEY, None, None, "custom");
        
        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(ENDPOINT_URL)
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

/// 同步下载文本
pub fn download_text_sync(url: &str) -> Result<String, reqwest::Error> {
    use reqwest::blocking::Client;
    let response = Client::new().get(url).send()?;
    response.text()
}