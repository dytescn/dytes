use crate::shared::credentials::{effective_s3, ResolvedS3};
use anyhow::{anyhow, Context, Result};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::{Config, Region};
use aws_sdk_s3::Client;
use futures_util::StreamExt;
use reqwest;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs::{create_dir_all, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::sleep;

/// 判断是否为 S3 对象路径（相对路径，且以已知包目录开头）
fn is_s3_path(path: &str) -> bool {
    !path.starts_with("http://")
        && !path.starts_with("https://")
        && (path.starts_with("core/")
            || path.starts_with("app/")
            || path.starts_with("system/")
            || path.starts_with("webs/")
            || path.starts_with("design_plugins/")
            || path.starts_with("design_soft/")
            || path.starts_with("models/"))
}

const FETCH_RETRIES: u32 = 3;

fn s3_settings() -> ResolvedS3 {
    effective_s3()
}

fn bucket() -> String {
    s3_settings().bucket
}

/// 读取远程文本内容（自动识别 S3 或 HTTP，带重试）。
///
/// 用于 dytes.json / packages.json 这类小配置，避免为了读一个 JSON
/// 先落盘到临时目录再读出来。
pub async fn fetch_text(path_or_url: &str) -> Result<String> {
    let mut last_error = String::new();

    for attempt in 1..=FETCH_RETRIES {
        let result = if is_s3_path(path_or_url) {
            fetch_s3_text(path_or_url).await
        } else {
            fetch_http_text(path_or_url).await
        };

        match result {
            Ok(text) => return Ok(text),
            Err(e) => {
                last_error = format!("{}", e);
                if attempt < FETCH_RETRIES {
                    println!(
                        "  读取失败 (尝试 {}/{}): {}，稍后重试...",
                        attempt, FETCH_RETRIES, e
                    );
                    sleep(Duration::from_secs(attempt as u64)).await;
                }
            }
        }
    }

    Err(anyhow!("读取 {} 失败: {}", path_or_url, last_error))
}

async fn fetch_s3_text(object_name: &str) -> Result<String> {
    let client = s3_client().await;

    let response = client
        .get_object()
        .bucket(bucket())
        .key(object_name)
        .send()
        .await
        .with_context(|| format!("S3 对象读取失败: {}", object_name))?;

    let bytes = response.body.collect().await.context("读取 S3 响应失败")?;
    String::from_utf8(bytes.into_bytes().to_vec())
        .with_context(|| format!("{} 不是有效的 UTF-8 文本", object_name))
}

async fn fetch_http_text(url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .context("构建 HTTP 客户端失败")?;

    let clean_url = url.split('?').next().unwrap_or(url);
    let response = client
        .get(clean_url)
        .send()
        .await
        .with_context(|| format!("HTTP 请求失败: {}", clean_url))?;

    if !response.status().is_success() {
        return Err(anyhow!("HTTP 错误状态码: {}", response.status()));
    }

    response.text().await.context("读取 HTTP 响应失败")
}

/// 创建 S3 客户端
async fn s3_client() -> Client {
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

    Client::from_conf(config)
}

/// 列出 S3 指定前缀下的对象 key
pub async fn list_keys(prefix: &str) -> Result<Vec<String>, String> {
    let client = s3_client().await;
    let mut keys = Vec::new();
    let mut continuation: Option<String> = None;

    loop {
        let mut req = client.list_objects_v2().bucket(bucket()).prefix(prefix);
        if let Some(token) = &continuation {
            req = req.continuation_token(token.clone());
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("ListObjectsV2 {} 失败: {}", prefix, e))?;

        for obj in resp.contents() {
            if let Some(key) = obj.key() {
                keys.push(key.to_string());
            }
        }

        if resp.is_truncated().unwrap_or(false) {
            continuation = resp.next_continuation_token().map(|s| s.to_string());
            if continuation.is_none() {
                break;
            }
        } else {
            break;
        }
    }

    Ok(keys)
}

/// 下载文件（公共入口，自动识别 S3 或 HTTP，并带有重试）
pub async fn download_file(url_or_path: &str, output_dir: &str) -> bool {
    let is_s3 = is_s3_path(url_or_path);

    const MAX_RETRIES: u32 = 10;
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

/// 从 S3 下载文件（内部函数，返回 Result）。
/// 按 32 MB 分块下载，支持 .part 断点续传，避免单个长连接中断导致全量重下。
async fn download_from_s3(object_name: &str, output_dir: &str) -> Result<()> {
    println!("  从 S3 下载: {}", object_name);

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

    let head = client
        .head_object()
        .bucket(bucket())
        .key(object_name)
        .send()
        .await
        .context("S3 HEAD 请求失败")?;
    let total_size = head.content_length().unwrap_or(0) as u64;
    if total_size == 0 {
        return Err(anyhow!("S3 对象大小为 0，可能无效"));
    }

    let filename = Path::new(object_name)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let filename = if filename.is_empty() {
        object_name.split('/').next_back().unwrap_or(object_name)
    } else {
        &filename
    };
    let dest_path = Path::new(output_dir).join(filename);
    let temp_path = dest_path.with_extension("part");
    if let Some(parent) = dest_path.parent() {
        create_dir_all(parent).await.context("创建输出目录失败")?;
    }

    let mut offset = std::fs::metadata(&temp_path).map(|m| m.len()).unwrap_or(0);
    if offset > total_size {
        offset = 0;
        let _ = tokio::fs::remove_file(&temp_path).await;
    }
    if offset > 0 {
        println!(
            "  检测到未完成文件，从 {} / {} 字节继续",
            offset, total_size
        );
    }

    let mut file = if offset > 0 {
        OpenOptions::new()
            .append(true)
            .open(&temp_path)
            .await
            .context("打开临时文件失败")?
    } else {
        File::create(&temp_path).await.context("创建临时文件失败")?
    };

    const CHUNK_SIZE: u64 = 32 * 1024 * 1024;
    while offset < total_size {
        let end = std::cmp::min(offset + CHUNK_SIZE - 1, total_size - 1);
        let response = client
            .get_object()
            .bucket(bucket())
            .key(object_name)
            .range(format!("bytes={}-{}", offset, end))
            .send()
            .await
            .with_context(|| format!("S3 Range GET 失败: {}-{}", offset, end))?;

        let mut stream = response.body.into_async_read();
        let mut buffer = [0u8; 8192];
        let mut chunk_written: u64 = 0;
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
            chunk_written += n as u64;
        }
        if chunk_written == 0 {
            file.flush().await?;
            drop(file);
            return Err(anyhow!("S3 Range 响应为空: {}-{}", offset, end));
        }
        offset += chunk_written;

        let progress = (offset as f32 / total_size as f32) * 100.0;
        println!("  下载进度: {:.1}% ({}/{})", progress, offset, total_size);
    }

    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    if offset != total_size {
        return Err(anyhow!(
            "下载不完整: 预期 {} 字节，实际 {} 字节",
            total_size,
            offset
        ));
    }

    tokio::fs::rename(&temp_path, &dest_path)
        .await
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
    let response = client
        .get(clean_url)
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
        create_dir_all(parent).await.context("创建输出目录失败")?;
    }

    let mut file = File::create(&temp_path).await.context("创建临时文件失败")?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("下载数据块失败")?;
        file.write_all(&chunk).await.context("写入临时文件失败")?;
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
            total_size,
            downloaded
        ));
    }

    tokio::fs::rename(&temp_path, &dest_path)
        .await
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
