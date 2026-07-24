use reqwest;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tokio::fs as tokio_fs;
use futures_util::StreamExt;

// 同步上传文件（用于 publish 工具）
pub fn upload_file(file_path: &str, bucket_name: &str, object_name: &str) -> bool {
    use std::process::Command;
    
    // 使用 aws cli 或直接使用 boto3 风格的调用
    // 这里使用简单的 HTTP PUT 方式
    let endpoint_url = "https://s3.api.upyun.com";
    let access_key = "195cc6d12c2d45389a5e52e5138f7250";
    let secret_key = "7ab145ae97310ca9f65db3dff07beac2";
    
    // 使用 aws s3api 命令行工具（如果没有安装，可以改用 reqwest）
    let status = Command::new("aws")
        .args([
            "s3api",
            "put-object",
            "--endpoint-url", endpoint_url,
            "--bucket", bucket_name,
            "--key", object_name,
            "--body", file_path,
            "--acl", "public-read",
        ])
        .env("AWS_ACCESS_KEY_ID", access_key)
        .env("AWS_SECRET_ACCESS_KEY", secret_key)
        .status();
    
    match status {
        Ok(s) if s.success() => {
            println!("  文件上传成功: {}", object_name);
            true
        }
        Ok(s) => {
            eprintln!("  文件上传失败: {}", s);
            false
        }
        Err(e) => {
            eprintln!("  执行 aws 命令失败: {}", e);
            false
        }
    }
}

pub async fn download_file(url: &str, output_dir: &str) -> bool {
    println!("  开始下载: {}", url);
    
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

pub async fn download_text(url: &str) -> Result<String, reqwest::Error> {
    let response = reqwest::get(url).await?;
    response.text().await
}

pub fn get_random_param() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("_t={}", timestamp)
}

/// 同步下载文本（用于 publish 工具）
pub fn download_text_sync(url: &str) -> Result<String, reqwest::Error> {
    use reqwest::blocking::Client;
    let response = Client::new().get(url).send()?;
    response.text()
}