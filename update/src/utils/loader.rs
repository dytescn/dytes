use reqwest;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tokio::fs as tokio_fs;
use futures_util::StreamExt;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn download_file(url: &str, output_dir: &str) -> bool {
    println!("  开始下载: {}", url);
    
    // 创建 HTTP 客户端
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("创建 HTTP 客户端失败: {}", e);
                return false;
            }
        };
    
    // 从 URL 中去除查询参数，用于提取文件名和发送请求
    let clean_url = url.split('?').next().unwrap_or(url);
    
    // 发送请求（使用干净 URL，不带查询参数）
    let response = match client.get(clean_url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("请求失败: {}", e);
            return false;
        }
    };
    
    // 检查状态码
    if !response.status().is_success() {
        eprintln!("HTTP 错误: {}", response.status());
        return false;
    }
    
    let total_size = response.content_length().unwrap_or(0);
    
    // 从干净 URL 中提取文件名（不含查询参数）
    let filename = Path::new(clean_url)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    
    let dest_path = Path::new(output_dir).join(filename.as_ref());
    
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
    
    // 下载数据流 - 使用 bytes_stream
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
        
        // 显示进度
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