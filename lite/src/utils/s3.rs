//! 轻量只读 S3 客户端（GET / HEAD + Range GET），实现 AWS Signature V4。
//!
//! 用于 Lite 版本，替代 `aws-sdk-s3`：只支持读取对象，不支持上传 / 列举。
//! `publish` 等写操作在 Lite 中不提供。
//!
//! 关键约束：请求使用 **path-style**（`{endpoint}/{bucket}/{key}`），与又拍云
//! S3 兼容层一致；签名只覆盖 `host` / `x-amz-content-sha256` / `x-amz-date`。

use crate::utils::credentials::{effective_s3, ResolvedS3};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::{Client, Method, Response, Url};
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn settings() -> ResolvedS3 {
    effective_s3()
}

pub fn bucket() -> String {
    settings().bucket
}

/// 未配置凭证时给用户的可操作提示。
const MISSING_CREDENTIALS: &str = "S3 凭证未配置：请执行 `dytes config set s3.access_key <AK>` \
     与 `dytes config set s3.secret_key <SK>`，或设置环境变量 \
     DYTES_S3_ACCESS_KEY / DYTES_S3_SECRET_KEY；本地构建要内置凭证，需在 \
     `cargo build` 时设置 UPYUN_API_KEY / UPYUN_API_SECRET";

fn is_blank(value: &Option<String>) -> bool {
    value.as_deref().map(str::trim).is_none_or(str::is_empty)
}

fn credentials_missing(settings: &ResolvedS3) -> bool {
    is_blank(&settings.access_key) || is_blank(&settings.secret_key)
}

/// 读取 / 下载前检查凭证，避免把「没有凭证」报成含糊的 `service error`。
pub fn ensure_credentials() -> Result<(), String> {
    if credentials_missing(&settings()) {
        Err(MISSING_CREDENTIALS.to_string())
    } else {
        Ok(())
    }
}

/// 可重试错误标记：超时 / 连接失败 / 5xx。
#[derive(Debug)]
pub struct RetryableError(anyhow::Error);

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

pub(crate) fn retryable(error: anyhow::Error) -> anyhow::Error {
    anyhow::Error::new(RetryableError(error))
}

/// 空 body 的 SHA-256（GET / HEAD 没有请求体）。
const EMPTY_PAYLOAD_SHA256: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn object_url(settings: &ResolvedS3, key: &str) -> Result<Url> {
    let endpoint = settings.endpoint.trim_end_matches('/');
    let url = format!("{endpoint}/{}/{}", settings.bucket, encode_path(key));
    Url::parse(&url).with_context(|| format!("S3 地址无效: {url}"))
}

fn host_header(url: &Url) -> String {
    match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    }
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for index in 0..BLOCK {
        ipad[index] ^= block[index];
        opad[index] ^= block[index];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 生成 SigV4 认证头；返回 `(x-amz-date, authorization)`。
fn sign(
    settings: &ResolvedS3,
    method: &str,
    url: &Url,
    payload_hash: &str,
    now: DateTime<Utc>,
) -> Result<(String, String)> {
    let access_key = settings.access_key.as_deref().unwrap_or_default().trim();
    let secret_key = settings.secret_key.as_deref().unwrap_or_default().trim();
    if access_key.is_empty() || secret_key.is_empty() {
        return Err(anyhow!("S3 凭证未配置"));
    }

    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date = now.format("%Y%m%d").to_string();
    let scope = format!("{date}/{}/s3/aws4_request", settings.region);
    let host = host_header(url);
    let canonical_headers = format!(
        "host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
    );
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";

    let canonical_request = format!(
        "{method}\n{}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        url.path()
    );
    let canonical_hash = hex(&Sha256::digest(canonical_request.as_bytes()));
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{canonical_hash}");

    let secret = format!("AWS4{secret_key}");
    let date_key = hmac_sha256(secret.as_bytes(), date.as_bytes());
    let region_key = hmac_sha256(&date_key, settings.region.as_bytes());
    let service_key = hmac_sha256(&region_key, b"s3");
    let signing_key = hmac_sha256(&service_key, b"aws4_request");
    let signature = hex(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );
    Ok((amz_date, authorization))
}

fn client() -> Result<Client> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .context("构建 HTTP 客户端失败")
}

fn classify_send_error(error: reqwest::Error, key: &str) -> anyhow::Error {
    let detail = format!("{error}");
    let message = anyhow!("S3 请求失败: {key} ({detail})");
    if error.is_timeout() || error.is_connect() || error.is_body() {
        retryable(message)
    } else {
        message
    }
}

fn classify_status(status: reqwest::StatusCode, key: &str) -> anyhow::Error {
    if status == reqwest::StatusCode::NOT_FOUND {
        anyhow!("S3 对象不存在: {key}")
    } else if status.is_server_error() {
        retryable(anyhow!("S3 请求失败: {key} (HTTP {status})"))
    } else {
        anyhow!("S3 请求失败: {key} (HTTP {status})")
    }
}

async fn send(
    method: Method,
    key: &str,
    range: Option<(u64, u64)>,
) -> Result<Response> {
    let settings = settings();
    let url = object_url(&settings, key)?;
    let payload_hash = EMPTY_PAYLOAD_SHA256;

    let mut request = client()?.request(method.clone(), url.clone());
    if !credentials_missing(&settings) {
        let now = Utc::now();
        let (amz_date, authorization) = sign(&settings, method.as_str(), &url, payload_hash, now)?;
        request = request
            .header("x-amz-date", amz_date)
            .header("x-amz-content-sha256", payload_hash)
            .header("authorization", authorization);
    }
    if let Some((start, end)) = range {
        request = request.header("range", format!("bytes={start}-{end}"));
    }

    let response = request
        .send()
        .await
        .map_err(|error| classify_send_error(error, key))?;
    if !response.status().is_success() {
        return Err(classify_status(response.status(), key));
    }
    Ok(response)
}

/// 读取对象为文本（用于 dytes.json / packages.json）。
pub async fn get_text(key: &str) -> Result<String> {
    let response = send(Method::GET, key, None).await?;
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("读取 S3 响应失败: {key}"))?;
    String::from_utf8(bytes.to_vec()).with_context(|| format!("{key} 不是有效的 UTF-8 文本"))
}

/// 读取对象大小（HEAD）。
pub async fn head_size(key: &str) -> Result<u64> {
    let response = send(Method::HEAD, key, None).await?;
    Ok(response.content_length().unwrap_or(0))
}

/// 发起 Range GET，返回响应供调用方流式写入文件。
pub async fn get_range(key: &str, start: u64, end: u64) -> Result<Response> {
    send(Method::GET, key, Some((start, end))).await
}

/// 下载对象到文件（小文件场景；大文件由 loader 分块调用 `get_range`）。
pub async fn download_to(key: &str, dest: &Path) -> Result<()> {
    let response = send(Method::GET, key, None).await?;
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("读取 S3 响应失败: {key}"))?;
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(dest, &bytes)
        .await
        .with_context(|| format!("写入文件失败: {}", dest.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(access_key: Option<&str>, secret_key: Option<&str>) -> ResolvedS3 {
        ResolvedS3 {
            endpoint: "https://example.com".to_string(),
            bucket: "bucket".to_string(),
            region: "us-east-1".to_string(),
            access_key: access_key.map(str::to_string),
            secret_key: secret_key.map(str::to_string),
        }
    }

    #[test]
    fn detects_missing_credentials() {
        assert!(!credentials_missing(&resolved(Some("AK"), Some("SK"))));
        assert!(credentials_missing(&resolved(None, Some("SK"))));
        assert!(credentials_missing(&resolved(Some("AK"), None)));
        assert!(credentials_missing(&resolved(Some(" "), Some("SK"))));
        assert!(credentials_missing(&resolved(Some("AK"), Some(""))));
    }

    #[test]
    fn encodes_object_path() {
        assert_eq!(encode_path("core/dytes/a.bin"), "core/dytes/a.bin");
        assert_eq!(encode_path("a b/中文"), "a%20b/%E4%B8%AD%E6%96%87");
    }

    #[test]
    fn builds_path_style_url() {
        let settings = resolved(Some("AK"), Some("SK"));
        let url = object_url(&settings, "core/dytes/dytes.json").unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.com/bucket/core/dytes/dytes.json"
        );
    }

    #[test]
    fn signs_deterministically() {
        let settings = resolved(Some("AKID"), Some("SECRET"));
        let url = Url::parse("https://example.com/bucket/core/dytes/dytes.json").unwrap();
        let now = DateTime::parse_from_rfc3339("2026-11-28T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (date, auth) = sign(&settings, "GET", &url, EMPTY_PAYLOAD_SHA256, now).unwrap();
        assert_eq!(date, "20261128T000000Z");
        assert!(auth.starts_with("AWS4-HMAC-SHA256 Credential=AKID/20261128/us-east-1/s3/aws4_request"));
        assert!(auth.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"));
        assert!(auth.contains("Signature="));
    }
}
