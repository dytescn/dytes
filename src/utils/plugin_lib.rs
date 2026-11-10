//! 通过 FFI 调用 `plugin_lib`（sys 包）的插件安装与更新。

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

/// 安装插件包到插件数据库。
pub fn install(app_dir: &Path, package_dir: &Path) -> Result<Value> {
    call(app_dir, "install_plugin", package_dir)
}

/// 更新已安装插件。
pub fn update(app_dir: &Path, package_dir: &Path) -> Result<Value> {
    call(app_dir, "update_plugin", package_dir)
}

fn call(app_dir: &Path, symbol: &str, package_dir: &Path) -> Result<Value> {
    let package_dir = std::path::absolute(package_dir).context("解析插件目录失败")?;
    let request = json!({ "dir": package_dir.to_string_lossy() });
    super::ffi::call(app_dir, "plugin_lib", symbol, &request, false)
}
