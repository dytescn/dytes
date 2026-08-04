use reqwest;
use tokio::fs::{File, create_dir_all};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use futures_util::StreamExt;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Region, Config};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use anyhow::{Result, anyhow, Context};
use tokio::time::sleep;

const ENDPOINT_URL: &str = "https://s3.api.upyun.com";
const ACCESS_KEY: &str = "195cc6d12c2d45389a5e52e5138f7250";
const SECRET_KEY: &str = "7ab145ae97310ca9f65db3dff07beac2";
const BUCKET_NAME: &str = "packages";

/// 下载文件（公共入口，自动识别 S3 或 HTTP，并带有重试）
pub async fn download_file(url_or_path: &str, output_dir: &str) -> bool {
    let is_s3 = !url_or_path.starts_with("http://")
        && !url_or_path.starts_with("https://")
        && (url_or_path.starts_with("core/")
            || url_or_path.starts_with("app/")
            || url_or_path.starts_with("system/")
            || url_or_path.starts_with("webs/"));

    const MAX_RETRIES: u32 = 3;
    let mut attempt = 0;
    let mut last_error = String::new();

    while attempt < MAX_RETRIES {
        if attempt > 0 {
            let delay = Duration::from_secs(2_u64.pow(attempt));
            println!("  等待 {} 秒后重试...", delay.as_secs());
            sleep(delay).await;
        }

        let result = if is_s3 {
            download_from_s3(url_or_path, output_dir).await
        } else {
            download_from_http(url_or_path, output_dir).await
        };

        match result {
            Ok(()) => return true,
            Err(e) => {
                last_error = format!("{}", e);
                eprintln!("  下载失败 (尝试 {}/{}): {}", attempt + 1, MAX_RETRIES, e);
                if !is_retryable_error(&e) {
                    break;
                }
                attempt += 1;
            }
        }
    }

    eprintln!("  下载最终失败: {}", last_error);
    false
}

fn is_retryable_error(e: &anyhow::Error) -> bool {
    if let Some(req_err) = e.downcast_ref::<reqwest::Error>() {
        if req_err.is_timeout() || req_err.is_connect() || req_err.is_body() {
            return true;
        }
        if let Some(status) = req_err.status() {
            if status.is_server_error() {
                return true;
            }
        }
    }
    false
}

/// 从 S3 下载文件（内部函数，返回 Result）
async fn download_from_s3(object_name: &str, output_dir: &str) -> Result<()> {
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

    let response = client
        .get_object()
        .bucket(BUCKET_NAME)
        .key(object_name)
        .send()
        .await
        .context("S3 GET 请求失败")?;

    // 修复：将 content_length() 返回的 i64 转为 u64，以便与 downloaded (u64) 比较
    let total_size = response.content_length().unwrap_or(0) as u64;
    if total_size == 0 {
        return Err(anyhow!("S3 对象大小为 0，可能无效"));
    }

    let filename = Path::new(object_name)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let filename = if filename.is_empty() {
        object_name.split('/').last().unwrap_or(object_name)
    } else {
        &filename
    };

    let dest_path = Path::new(output_dir).join(filename);
    let temp_path = dest_path.with_extension("part");

    if let Some(parent) = dest_path.parent() {
        create_dir_all(parent).await
            .context("创建输出目录失败")?;
    }

    let mut file = File::create(&temp_path).await
        .context("创建临时文件失败")?;

    let mut stream = response.body.into_async_read();
    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 8192];

    loop {
        let n = stream.read(&mut buffer).await
            .context("读取 S3 数据流失败")?;
        if n == 0 { break; }
        file.write_all(&buffer[..n]).await
            .context("写入临时文件失败")?;
        downloaded += n as u64;

        if total_size > 0 {
            let progress = (downloaded as f32 / total_size as f32) * 100.0;
            print!("\r  下载进度: {:.1}%", progress);
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
    }

    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    println!();

    if downloaded != total_size {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(anyhow!(
            "下载不完整: 预期 {} 字节，实际 {} 字节",
            total_size, downloaded
        ));
    }

    tokio::fs::rename(&temp_path, &dest_path).await
        .context("重命名临时文件失败")?;

    println!("  下载完成: {}", dest_path.display());
    Ok(())
}

/// 从 HTTP 下载文件（内部函数，返回 Result）
async fn download_from_http(url: &str, output_dir: &str) -> Result<()> {
    println!("  从 HTTP 下载: {}", url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .context("构建 HTTP 客户端失败")?;

    let clean_url = url.split('?').next().unwrap_or(url);
    let response = client.get(clean_url)
        .send()
        .await
        .context("HTTP 请求失败")?;

    if !response.status().is_success() {
        return Err(anyhow!("HTTP 错误状态码: {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(0);
    if total_size == 0 {
        return Err(anyhow!("HTTP 响应 Content-Length 为 0"));
    }

    let filename = Path::new(clean_url)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let dest_path = Path::new(output_dir).join(filename.as_ref());
    let temp_path = dest_path.with_extension("part");

    if let Some(parent) = dest_path.parent() {
        create_dir_all(parent).await
            .context("创建输出目录失败")?;
    }

    let mut file = File::create(&temp_path).await
        .context("创建临时文件失败")?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("下载数据块失败")?;
        file.write_all(&chunk).await
            .context("写入临时文件失败")?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let progress = (downloaded as f32 / total_size as f32) * 100.0;
            print!("\r  下载进度: {:.1}%", progress);
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
    }

    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    println!();

    if downloaded != total_size {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(anyhow!(
            "下载不完整: 预期 {} 字节，实际 {} 字节",
            total_size, downloaded
        ));
    }

    tokio::fs::rename(&temp_path, &dest_path).await
        .context("重命名临时文件失败")?;

    println!("  下载完成: {}", dest_path.display());
    Ok(())
}

/// 生成随机参数（用于缓存破坏）
pub fn get_random_param() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("_t={}", timestamp)
}