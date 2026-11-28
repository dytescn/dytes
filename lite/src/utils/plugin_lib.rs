//! 通过 FFI 调用 `plugin_lib`（sys 包）的插件安装与更新。
//!
//! 插件包可自带 `setup.json`：安装 / 更新成功后按其请求执行，
//! 用于写入插件自己的配置（`library` / `symbol` 方式，由 dytes 直接调用 FFI）。

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

/// 安装插件包到插件数据库，并执行包内 `setup.json`。
pub fn install(app_dir: &Path, package_dir: &Path) -> Result<Value> {
    let package_dir = std::path::absolute(package_dir).context("解析插件目录失败")?;
    let result = call(app_dir, "install_plugin", &package_dir)?;
    run_setup(app_dir, &package_dir)?;
    Ok(result)
}

/// 更新已安装插件，并执行包内 `setup.json`。
pub fn update(app_dir: &Path, package_dir: &Path) -> Result<Value> {
    let package_dir = std::path::absolute(package_dir).context("解析插件目录失败")?;
    let result = call(app_dir, "update_plugin", &package_dir)?;
    run_setup(app_dir, &package_dir)?;
    Ok(result)
}

/// 执行插件包自带的 `setup.json`；没有该文件时跳过。
fn run_setup(app_dir: &Path, package_dir: &Path) -> Result<()> {
    let setup_path = package_dir.join("setup.json");
    if !setup_path.is_file() {
        return Ok(());
    }

    let setup = setup_path.to_string_lossy().to_string();
    let requests = crate::update::script::load(&setup)
        .with_context(|| format!("读取插件 setup.json 失败: {}", setup_path.display()))?;
    if requests.is_empty() {
        return Ok(());
    }

    println!(
        "  执行插件 setup.json: {}（{} 个请求）",
        setup_path.display(),
        requests.len()
    );
    crate::update::script::execute(app_dir, &requests)
        .with_context(|| format!("执行插件 setup.json 失败: {}", setup_path.display()))
}

fn call(app_dir: &Path, symbol: &str, package_dir: &Path) -> Result<Value> {
    let package_dir = std::path::absolute(package_dir).context("解析插件目录失败")?;
    let request = json!({ "dir": package_dir.to_string_lossy() });
    super::ffi::call(app_dir, "plugin_lib", symbol, &request, false)
}
