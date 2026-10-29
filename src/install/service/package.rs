//! 单包安装：复用 update 引擎的下载与安装流程。
//!
//! `dytes install plugin <name>` / `dytes install design-soft <name>` 都走
//! [`update_design::update_design_info`]，保证安装与更新使用同一套目录约定、
//! 锁文件与 libary 注册逻辑。

use anyhow::Result;
use serde_json::{json, Value};

use crate::update::run::update_design::{self, DesignKind};

pub fn install_design(
    name: &str,
    kind: DesignKind,
    version: Option<&str>,
    force: bool,
) -> Result<Value> {
    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime.block_on(update_design::update_design_info(
        name.to_string(),
        version.unwrap_or("latest").to_string(),
        kind,
        force,
        version,
    ))?;

    Ok(json!({
        "type": kind.as_str(),
        "name": result.name,
        "version": result.version,
        "path": result.path,
        "files": result.files,
        "skipped": result.skipped,
    }))
}
