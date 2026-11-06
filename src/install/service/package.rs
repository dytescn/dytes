//! 单包安装：复用 update 引擎的下载与安装流程。
//!
//! `dytes install plugin <name>` / `dytes install design-soft <name>` 都走
//! [`update_design::update_design_info`]，保证安装与更新使用同一套目录约定、
//! 锁文件与 libary 注册逻辑。

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::update::run::update_design::{self, DesignKind};
use crate::update::utils::dytes::DytesConfig;
use crate::update::utils::loader::fetch_text;

pub fn install_design(
    name: &str,
    kind: DesignKind,
    version: Option<&str>,
    force: bool,
) -> Result<Value> {
    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime.block_on(async {
        // 先按插件清单安装依赖；已安装且版本满足的包会跳过，
        // 未安装或落后的包安装线上最新版本，环上的回边会被跳过。
        let manifest_path = format!("{}/{}/dytes.json", kind.as_str(), name);
        let manifest_text = fetch_text(&manifest_path)
            .await
            .with_context(|| format!("获取 {} 失败", manifest_path))?;
        let manifest: DytesConfig = serde_json::from_str(&manifest_text)
            .with_context(|| format!("解析 {} 失败", manifest_path))?;
        crate::update::run::dytes::install_dependencies_for(&manifest).await?;

        let result = update_design::update_design_info(
            name.to_string(),
            version.unwrap_or("latest").to_string(),
            kind,
            force,
            version,
        )
        .await?;

        // 插件下载完成后触发 plugin_lib 扫描，登记 plugins.db 并导入 kv。
        if kind == DesignKind::Plugin {
            crate::update::run::update_setup::scan_plugin_registry().await?;
        }

        Ok::<_, anyhow::Error>(result)
    })?;

    Ok(json!({
        "type": kind.as_str(),
        "name": result.name,
        "uuid": result.uuid,
        "version": result.version,
        "path": result.path,
        "files": result.files,
        "skipped": result.skipped,
    }))
}
