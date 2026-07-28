use reqwest;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tokio::fs as tokio_fs;
use futures_util::StreamExt;
use std::time::{SystemTime, UNIX_EPOCH};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Region, Config};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;

const ENDPOINT_URL: &str = "https://s3.api.upyun.com";
const ACCESS_KEY: &str = "195cc6d12c2d45389a5e52e5138f7250";
const SECRET_KEY: &str = "7ab145ae97310ca9f65db3dff07beac2";
const BUCKET_NAME: &str = "packages";

/// 从 S3 下载文件
async fn download_from_s3(object_name: &str, output_dir: &str) -> bool {
    println!("  从 S3 下载: {}", object_name);
    
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
        .get_object()
        .bucket(BUCKET_NAME)
        .key(object_name)
        .send()
        .await;
    
    match result {
        Ok(response) => {
            let total_size = response.content_length().unwrap_or(0);
            
            // 从 object_name 中提取文件名
            let filename = Path::new(object_name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            
            // 如果文件名是空的，使用 object_name 的最后一部分
            let filename = if filename.is_empty() {
                object_name.split('/').last().unwrap_or(object_name)
            } else {
                &filename
            };
            
            let dest_path = Path::new(output_dir).join(filename);
            
            // 创建目录
            if let Some(parent) = dest_path.parent() {
                if let Err(e) = tokio_fs::create_dir_all(parent).await {
                    eprintln!("创建目录失败: {}", e);
                    return false;
                }
            }
            
            // 创建文件
            let mut file = match File::create(&dest_path) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("创建文件失败: {}", e);
                    return false;
                }
            };
            
            // 下载数据流
            let mut stream = response.body.into_async_read();
            let mut downloaded: u64 = 0;
            let mut buffer = [0u8; 8192];
            
            use tokio::io::AsyncReadExt;
            loop {
                match stream.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(e) = file.write_all(&buffer[..n]) {
                            eprintln!("写入文件失败: {}", e);
                            return false;
                        }
                        downloaded += n as u64;
                        
                        // 显示进度
                        if total_size > 0 {
                            let progress = (downloaded as f32 / total_size as f32) * 100.0;
                            print!("\r  下载进度: {:.1}%", progress);
                            let _ = std::io::Write::flush(&mut std::io::stdout());
                        }
                    }
                    Err(e) => {
                        eprintln!("读取数据失败: {}", e);
                        return false;
                    }
                }
            }
            
            println!("\n  下载完成: {}", dest_path.display());
            true
        }
        Err(e) => {
            eprintln!("S3 下载失败: {}", e);
            false
        }
    }
}

/// 下载文件（自动识别 S3 路径或 HTTP URL）
pub async fn download_file(url_or_path: &str, output_dir: &str) -> bool {
    println!("  开始下载: {}", url_or_path);
    
    // 判断是否是 S3 路径（以 core/、app/、system/、webs/ 开头，不包含 http）
    let is_s3_path = !url_or_path.starts_with("http://") 
        && !url_or_path.starts_with("https://")
        && (url_or_path.starts_with("core/") 
            || url_or_path.starts_with("app/") 
            || url_or_path.starts_with("system/") 
            || url_or_path.starts_with("webs/"));
    
    if is_s3_path {
        // 从 S3 下载
        download_from_s3(url_or_path, output_dir).await
    } else {
        // 从 HTTP 下载（保留原有逻辑）
        download_from_http(url_or_path, output_dir).await
    }
}

/// 从 HTTP 下载文件
async fn download_from_http(url: &str, output_dir: &str) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("创建 HTTP 客户端失败: {}", e);
                return false;
            }
        };
    
    let clean_url = url.split('?').next().unwrap_or(url);
    
    let response = match client.get(clean_url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("请求失败: {}", e);
            return false;
        }
    };
    
    if !response.status().is_success() {
        eprintln!("HTTP 错误: {}", response.status());
        return false;
    }
    
    let total_size = response.content_length().unwrap_or(0);
    let filename = Path::new(clean_url)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let dest_path = Path::new(output_dir).join(filename.as_ref());
    
    if let Some(parent) = dest_path.parent() {
        if let Err(e) = tokio_fs::create_dir_all(parent).await {
            eprintln!("创建目录失败: {}", e);
            return false;
        }
    }
    
    let mut file = match File::create(&dest_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("创建文件失败: {}", e);
            return false;
        }
    };
    
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                eprintln!("下载数据失败: {}", e);
                return false;
            }
        };
        
        if let Err(e) = file.write_all(&chunk) {
            eprintln!("写入文件失败: {}", e);
            return false;
        }
        
        downloaded += chunk.len() as u64;
        
        if total_size > 0 {
            let progress = (downloaded as f32 / total_size as f32) * 100.0;
            print!("\r  下载进度: {:.1}%", progress);
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
    }
    
    println!("\n  下载完成: {}", dest_path.display());
    true
}

pub fn get_random_param() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("_t={}", timestamp)
}