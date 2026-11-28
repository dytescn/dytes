//! 通用库 FFI 调用：加载 `libary/<name>` 并按 JSON 调用导出符号。
//!
//! setup.json 中的请求与插件安装共用同一套调用约定：
//! 输入为 JSON 字符串，输出为 `{code, data, msg}`，`code == 200` 表示成功。

use anyhow::{bail, Context, Result};
use libloading::Library;
use serde_json::Value;
use std::ffi::{c_char, c_int, CStr, CString};
use std::path::{Path, PathBuf};

/// 与各 libary 的输出缓冲区约定一致（64 KiB）。
const OUT_BUF_SIZE: usize = 65_536;

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

/// 解析 `libary/<name>` 的实际文件路径。
pub fn library_path(app_dir: &Path, library: &str) -> Result<PathBuf> {
    let candidates = library_candidates(library);
    let dir = app_dir.join("libary");
    for name in &candidates {
        let path = dir.join(name);
        if path.is_file() {
            return Ok(path);
        }
    }
    bail!(
        "未找到 libary/{library}（尝试: {}）",
        candidates.join(", ")
    )
}

fn library_candidates(library: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![format!("{library}.dll")]
    }
    #[cfg(target_os = "linux")]
    {
        vec![format!("{library}.so"), format!("lib{library}.so")]
    }
    #[cfg(target_os = "macos")]
    {
        vec![format!("{library}.dylib"), format!("lib{library}.dylib")]
    }
}

/// 导出符号候选：优先原样，其次按 cyctron 约定补 `post_` 前缀。
fn symbol_candidates(symbol: &str) -> Vec<String> {
    let symbol = symbol.trim();
    let mut candidates = vec![symbol.to_string()];
    if !symbol.starts_with("post_") {
        candidates.push(format!("post_{symbol}"));
    }
    candidates
}

/// 调用库导出符号。
///
/// `allow_error` 为真时，业务码非 200 只打印警告并返回 `Null`；
/// 库缺失、符号缺失、指针或编码错误始终视为失败。
pub fn call(
    app_dir: &Path,
    library: &str,
    symbol: &str,
    request: &Value,
    allow_error: bool,
) -> Result<Value> {
    let app_dir = std::path::absolute(app_dir).context("解析应用目录失败")?;
    let lib_path = library_path(&app_dir, library)?;

    let _cwd = CwdGuard::switch_to(&app_dir)?;

    // SAFETY: 库由本应用安装，符号签名与各 libary 的 C ABI 一致。
    let handle = unsafe { Library::new(&lib_path) }
        .with_context(|| format!("加载 {} 失败", lib_path.display()))?;

    let request = CString::new(request.to_string()).context("FFI 请求包含 NUL")?;
    let mut output = vec![0u8; OUT_BUF_SIZE];

    // 兼容 cyctron 的符号命名：setup.json 写 `db_update`，实际导出是 `post_db_update`。
    let candidates = symbol_candidates(symbol);
    let mut function = None;
    for candidate in &candidates {
        if let Ok(found) = unsafe { handle.get::<FfiFn>(candidate.as_bytes()) } {
            function = Some(found);
            break;
        }
    }
    let function = function.ok_or_else(|| {
        anyhow::anyhow!(
            "{} 缺少符号 {}（尝试: {}）",
            lib_path.display(),
            symbol,
            candidates.join(", ")
        )
    })?;

    // SAFETY: 入参为 NUL 结尾的 JSON，出参缓冲区符合 64 KiB 约定。
    let ret = unsafe { function(request.as_ptr(), output.as_mut_ptr() as *mut c_char) };
    if ret != 0 {
        bail!("{library}:{symbol} 返回 {ret}（输出缓冲区不足或指针为空）");
    }

    let text = unsafe { CStr::from_ptr(output.as_ptr() as *const c_char) }
        .to_str()
        .context("FFI 响应不是 UTF-8")?;
    let response: Value =
        serde_json::from_str(text).with_context(|| format!("解析 FFI 响应失败: {text}"))?;

    let code = response.get("code").and_then(Value::as_i64).unwrap_or(0);
    if code != 200 {
        let message = response
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("未知错误");
        if allow_error {
            eprintln!("  忽略业务错误（allow_error）: {library}:{symbol} code {code} - {message}");
            return Ok(Value::Null);
        }
        bail!("{library}:{symbol} 失败（code {code}）: {message}");
    }

    Ok(response.get("data").cloned().unwrap_or(Value::Null))
}

/// 临时切换工作目录，离开作用域时恢复。
struct CwdGuard(Option<PathBuf>);

impl CwdGuard {
    fn switch_to(dir: &Path) -> Result<Self> {
        let previous = std::env::current_dir().ok();
        std::env::set_current_dir(dir)
            .with_context(|| format!("切换目录失败: {}", dir.display()))?;
        Ok(Self(previous))
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            let _ = std::env::set_current_dir(previous);
        }
    }
}
