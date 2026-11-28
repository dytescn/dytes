//! `setup.json`：读取初始化请求，并通过 libary 的 FFI 直接执行。
//!
//! 请求格式：
//! ```json
//! {
//!   "requests": [
//!     {
//!       "url": "http://127.0.0.1:44944/database",
//!       "headers": { "FFI-Symbol": "db_update" },
//!       "body": { "path": "./plugins", "sql": "CREATE TABLE ..." },
//!       "allow_error": true
//!     }
//!   ]
//! }
//! ```
//!
//! 目标库默认从 `url` 最后一段推断（`/database` → `database`），
//! 符号取 `FFI-Symbol` 请求头；也可以通过 `library` / `symbol` 字段显式指定。

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct SetupFile {
    #[serde(default)]
    pub requests: Vec<SetupRequest>,
}

#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<Value>,
    #[serde(default)]
    pub allow_error: bool,
    /// 可选：直接指定目标库，缺省从 `url` 推断。
    #[serde(default)]
    pub library: Option<String>,
    /// 可选：直接指定导出符号，缺省读取 `FFI-Symbol` 头。
    #[serde(default)]
    pub symbol: Option<String>,
}

/// 读取 `setup.json` 的请求列表。
pub fn load(path: &str) -> Result<Vec<SetupRequest>> {
    let text = fs::read_to_string(path).with_context(|| format!("读取 {path} 失败"))?;
    let file: SetupFile =
        serde_json::from_str(&text).with_context(|| format!("解析 {path} 失败"))?;
    Ok(file.requests)
}

/// 按顺序通过 FFI 执行请求。
pub fn execute(app_dir: &Path, requests: &[SetupRequest]) -> Result<()> {
    for (index, request) in requests.iter().enumerate() {
        let library = library_of(request)?;
        let symbol = symbol_of(request)?;
        let body = request
            .body
            .clone()
            .unwrap_or_else(|| Value::Object(Default::default()));

        println!(
            "  [setup {}/{}] {} -> {}:{}",
            index + 1,
            requests.len(),
            request.url,
            library,
            symbol
        );
        crate::utils::ffi::call(app_dir, &library, &symbol, &body, request.allow_error)?;
    }
    Ok(())
}

fn library_of(request: &SetupRequest) -> Result<String> {
    if let Some(library) = request
        .library
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(library.to_string());
    }

    let path = request
        .url
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');
    let library = path.rsplit('/').next().unwrap_or("").trim();
    if library.is_empty() {
        bail!("setup 请求缺少目标库（url={}）", request.url);
    }
    Ok(library.to_string())
}

fn symbol_of(request: &SetupRequest) -> Result<String> {
    if let Some(symbol) = request
        .symbol
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(symbol.to_string());
    }

    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("ffi-symbol"))
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("setup 请求缺少 FFI-Symbol 头（url={}）", request.url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(url: &str, symbol: Option<&str>) -> SetupRequest {
        SetupRequest {
            url: url.to_string(),
            method: String::new(),
            headers: symbol
                .map(|value| HashMap::from([("FFI-Symbol".to_string(), value.to_string())]))
                .unwrap_or_default(),
            body: None,
            allow_error: false,
            library: None,
            symbol: None,
        }
    }

    #[test]
    fn derives_library_and_symbol_from_url_and_header() {
        let request = request("http://127.0.0.1:44944/database?x=1", Some("db_update"));
        assert_eq!(library_of(&request).unwrap(), "database");
        assert_eq!(symbol_of(&request).unwrap(), "db_update");
    }

    #[test]
    fn explicit_library_and_symbol_win() {
        let mut request = request("", None);
        request.library = Some("info_lib".into());
        request.symbol = Some("info_post".into());
        assert_eq!(library_of(&request).unwrap(), "info_lib");
        assert_eq!(symbol_of(&request).unwrap(), "info_post");
    }

    #[test]
    fn missing_target_is_an_error() {
        assert!(library_of(&request("", None)).is_err());
        assert!(symbol_of(&request("http://x/database", None)).is_err());
    }
}
