use crate::utils::s3;
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use reqwest;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::fs::{create_dir_all, File, OpenOptions};
use tokio::io::AsyncWriteExt;
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

/// 启动更新模式：只做一次尝试，避免网络/凭证异常时在启动链路上反复重试。
static STARTUP_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_startup_mode(enabled: bool) {
    STARTUP_MODE.store(enabled, Ordering::Relaxed);
}

fn startup_mode() -> bool {
    STARTUP_MODE.load(Ordering::Relaxed)
}

fn fetch_retries() -> u32 {
    if startup_mode() {
        1
    } else {
        FETCH_RETRIES
    }
}

/// 读取远程文本内容（自动识别 S3 或 HTTP，带重试）。
///
/// 用于 dytes.json / packages.json 这类小配置，避免为了读一个 JSON
/// 先落盘到临时目录再读出来。
pub async fn fetch_text(path_or_url: &str) -> Result<String> {
    if is_s3_path(path_or_url) {
        if let Err(hint) = s3::ensure_credentials() {
            return Err(anyhow!(
                "读取 {} 失败: S3 对象读取失败（{hint}）",
                path_or_url
            ));
        }
    }

    let mut last_error = String::new();
    let retries = fetch_retries();

    for attempt in 1..=retries {
        let result = if is_s3_path(path_or_url) {
            fetch_s3_text(path_or_url).await
        } else {
            fetch_http_text(path_or_url).await
        };

        match result {
            Ok(text) => return Ok(text),
            Err(e) => {
                last_error = format!("{}", e);
                if attempt < retries {
                    println!(
                        "  读取失败 (尝试 {}/{}): {}，稍后重试...",
                        attempt, retries, e
                    );
                    sleep(Duration::from_secs(attempt as u64)).await;
                }
            }
        }
    }

    Err(anyhow!("读取 {} 失败: {}", path_or_url, last_error))
}

async fn fetch_s3_text(object_name: &str) -> Result<String> {
    s3::get_text(object_name)
        .await
        .with_context(|| format!("S3 对象读取失败: {}", object_name))
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

/// 列出 S3 前缀下的对象（Lite 只读客户端未实现 ListObjectsV2）。
///
/// `update --discover` 会优先读 `packages.json`；只有索引缺失时才会走到这里。
pub async fn list_keys(prefix: &str) -> Result<Vec<String>, String> {
    Err(format!(
        "Lite 版本不支持列举对象（{prefix}）；请使用 packages.json 索引"
    ))
}

/// 下载文件（公共入口，自动识别 S3 或 HTTP，并带有重试）
pub async fn download_file(url_or_path: &str, output_dir: &str) -> bool {
    let is_s3 = is_s3_path(url_or_path);

    if is_s3 {
        if let Err(hint) = s3::ensure_credentials() {
            eprintln!("  下载最终失败: S3 下载失败: {url_or_path}（{hint}）");
            return false;
        }
    }

    const MAX_RETRIES: u32 = 10;
    let max_retries = if startup_mode() { 2 } else { MAX_RETRIES };
    let mut attempt = 0;
    let mut last_error = String::new();

    while attempt < max_retries {
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
                last_error = format!("{e:#}");
                eprintln!("  下载失败 (尝试 {}/{}): {e:#}", attempt + 1, max_retries);
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

/// 显式标记可重试错误。
///
/// `anyhow` 在错误被 `format!` 字符串化或经过多层 `context` 后会擦除原始类型，
/// 仅对最外层做 `downcast` 无法识别 S3 SDK 的临时性错误。用该标记承载重试意图，
/// 并在 `is_retryable_error` 中沿错误链查找。
#[derive(Debug)]
struct RetryableError(anyhow::Error);

impl std::fmt::Display for RetryableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#}", self.0)
    }
}

impl std::error::Error for RetryableError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

fn retryable_error(error: anyhow::Error) -> anyhow::Error {
    anyhow::Error::new(RetryableError(error))
}

fn is_retryable_error(e: &anyhow::Error) -> bool {
    for cause in e.chain() {
        if cause.downcast_ref::<RetryableError>().is_some()
            || cause.downcast_ref::<s3::RetryableError>().is_some()
        {
            return true;
        }
        if let Some(req_err) = cause.downcast_ref::<reqwest::Error>() {
            if req_err.is_timeout() || req_err.is_connect() || req_err.is_body() {
                return true;
            }
            if let Some(status) = req_err.status() {
                if status.is_server_error() {
                    return true;
                }
            }
        }
    }
    false
}

/// 从 S3 下载文件（内部函数，返回 Result）。
/// 按 32 MB 分块下载，支持 .part 断点续传，避免单个长连接中断导致全量重下。
async fn download_from_s3(object_name: &str, output_dir: &str) -> Result<()> {
    println!("  从 S3 下载: {}", object_name);

    s3::ensure_credentials().map_err(|hint| anyhow!("S3 下载失败: {object_name}（{hint}）"))?;

    let total_size = s3::head_size(object_name)
        .await
        .map_err(|error| {
            if is_retryable_error(&error) {
                retryable_error(error).context(format!("S3 HEAD 请求失败: {object_name}"))
            } else {
                error.context(format!("S3 HEAD 请求失败: {object_name}"))
            }
        })?;
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
        let response = s3::get_range(object_name, offset, end)
            .await
            .map_err(|error| {
                let message = error.context(format!("S3 Range GET 失败: {offset}-{end}"));
                if is_retryable_error(&message) {
                    retryable_error(message)
                } else {
                    message
                }
            })?;

        let mut stream = response.bytes_stream();
        let mut chunk_written: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|error| {
                let should_retry = error.is_timeout() || error.is_connect() || error.is_body();
                let error = anyhow::Error::new(error).context("读取 S3 数据流失败");
                if should_retry {
                    retryable_error(error)
                } else {
                    error
                }
            })?;
            if bytes.is_empty() {
                break;
            }
            file.write_all(&bytes)
                .await
                .context("写入临时文件失败")?;
            chunk_written += bytes.len() as u64;
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
        let status = response.status();
        let error = anyhow!("HTTP 错误状态码: {status}");
        return Err(if status.is_server_error() {
            retryable_error(error)
        } else {
            error
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_marker_survives_context() {
        let error = retryable_error(anyhow!("service error")).context("S3 HEAD 请求失败");
        assert!(is_retryable_error(&error));
    }

    #[test]
    fn plain_error_is_not_retryable() {
        assert!(!is_retryable_error(&anyhow!("plain failure")));
    }
}
