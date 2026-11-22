//! `dytes install` 命令入口。

use clap::Args;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

use crate::install::package as install_package;
use crate::utils::error::{DytesError, Result};
use crate::update::update_design::DesignKind;
use crate::utils::dytes::DependencyType;
use crate::utils::version::parse_name_version;

#[derive(Args)]
pub struct InstallArgs {
    /// 包类型：plugin / design-soft / sys / core / web / model（app 请使用 dytes update）
    #[arg(value_name = "TYPE")]
    pkg_type: Option<String>,
    /// 包名（可带 @version）
    #[arg(value_name = "NAME")]
    name: Option<String>,
    #[arg(long)]
    version: Option<String>,
    #[arg(long)]
    dir: Option<PathBuf>,
    #[arg(long)]
    gui: bool,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    launch: bool,
    #[arg(long = "no-init")]
    no_init: bool,
    #[arg(long)]
    offline: bool,
    #[arg(long)]
    json: bool,
}

pub fn run(args: InstallArgs) -> Result<()> {
    if args.gui {
        #[cfg(feature = "gui")]
        {
            return crate::utils::gui::run_install_gui(args.dir.clone(), args.launch)
                .map_err(|error| DytesError::failed(format!("{error:#}")));
        }
        #[cfg(not(feature = "gui"))]
        {
            return Err(DytesError::args(
                "当前构建未启用图形界面，请使用启用 `gui` feature 的构建",
            ));
        }
    }

    // 应用根：`--dir` 明确指定时以其为准；否则与更新引擎使用同一个根
    // （支持 `DYTES_APP_DIR`，没有时回退当前目录）。
    let requested_dir = match &args.dir {
        Some(dir) => dir.clone(),
        None => PathBuf::from(crate::update::env::get_app_dir()),
    };
    std::env::set_current_dir(&requested_dir)
        .map_err(|e| DytesError::failed(format!("切换目录失败: {e}")))?;
    // `--dir` 可能是相对路径；切换后再取一次绝对路径，避免后续拼接
    // `dytes.json` 时把 `app` 又拼成 `app/app`。
    let dir = std::env::current_dir()?;
    // 固定应用根，确保清单写入位置与文件安装位置一致。
    std::env::set_var("DYTES_APP_DIR", &dir);

    let pkg_type = args.pkg_type.as_deref().ok_or_else(|| {
        DytesError::args("请指定包类型，例如：dytes install plugin cdr_check_txt")
    })?;
    let (name, version) = parse_name_version(args.name.as_deref(), args.version.as_deref())
        .ok_or_else(|| DytesError::args("缺少包名"))?;

    let kind = match DependencyType::from_name(pkg_type) {
        Some(DependencyType::DesignPlugins) => DesignKind::Plugin,
        Some(DependencyType::DesignSoft) => DesignKind::Soft,
        Some(dep_type) => {
            return install_dependency(
                &dir,
                dep_type,
                &name,
                version.as_deref(),
                args.offline,
                args.json,
            )
        }
        None => match pkg_type.trim().to_ascii_lowercase().as_str() {
            "app" | "apps" => {
                return Err(DytesError::args(
                    "安装应用请使用 dytes update --auto（当前目录的 dytes.json 即应用清单）",
                ))
            }
            other => {
                return Err(DytesError::args(format!(
                    "install 暂不支持类型 {other}；支持 plugin / design-soft / sys / core / web / model"
                )))
            }
        },
    };

    let value = install_package::install_design(&name, kind, version.as_deref(), args.force)
        .map_err(|e| DytesError::failed(format!("安装失败: {e:#}")))?;

    if args.json {
        println!("{}", value);
    } else {
        let action = if value["skipped"].as_bool().unwrap_or(false) {
            "已是最新版本"
        } else {
            "已安装"
        };
        println!(
            "{action}: {} {} v{} -> {}",
            value["type"].as_str().unwrap_or(""),
            name,
            value["version"].as_str().unwrap_or(""),
            value["path"].as_str().unwrap_or("")
        );
    }

    Ok(())
}

async fn fetch_dependency_version(
    dep_type: DependencyType,
    name: &str,
) -> anyhow::Result<String> {
    let path = format!("{}/{}/dytes.json", dep_type.remote_dir(), name);
    let text = crate::update::loader::fetch_text(&path).await?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("解析 {path} 失败: {e}"))?;
    value
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("{path} 缺少 version"))
}

/// 把 `name@version` 写入清单的 `key` 分组：已有同名条目则更新，没有则新增。
fn upsert_dependency(
    manifest: &mut serde_json::Value,
    key: &str,
    name: &str,
    version: &str,
) -> Result<()> {
    let object = manifest
        .as_object_mut()
        .ok_or_else(|| DytesError::config("dytes.json 不是 JSON 对象"))?;
    let entry = object
        .entry(key.to_string())
        .or_insert_with(|| serde_json::json!({}));
    let deps = entry
        .as_object_mut()
        .ok_or_else(|| DytesError::config(format!("{key} 不是对象")))?;
    deps.insert(
        name.to_string(),
        serde_json::Value::String(version.to_string()),
    );
    Ok(())
}

/// 安装 sys / core / web / model 依赖：写入当前清单后执行 update。
fn install_dependency(
    dir: &Path,
    dep_type: DependencyType,
    name: &str,
    version: Option<&str>,
    offline: bool,
    json: bool,
) -> Result<()> {
    let key = dep_type.manifest_key();
    let manifest_path = dir.join("dytes.json");
    if !manifest_path.exists() {
        return Err(DytesError::config(format!(
            "{} 不存在；请先执行 dytes init 或在应用目录运行",
            manifest_path.display()
        )));
    }

    let text = fs::read_to_string(&manifest_path)
        .map_err(|e| DytesError::config(format!("读取 {} 失败: {e}", manifest_path.display())))?;
    let mut value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| DytesError::config(format!("解析 dytes.json 失败: {e}")))?;

    let version = match version {
        Some(version) => version.to_string(),
        None if offline => {
            return Err(DytesError::args(
                "离线安装需要显式版本：dytes install <type> <name>@<version> --offline",
            ))
        }
        None => {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| DytesError::failed(format!("创建异步运行时失败: {e}")))?;
            runtime
                .block_on(fetch_dependency_version(dep_type, name))
                .map_err(|e| DytesError::failed(format!("获取线上版本失败: {e:#}")))?
        }
    };

    // 已有同名依赖则更新到本次解析出的版本，没有则新增。
    upsert_dependency(&mut value, key, name, &version)?;
    fs::write(&manifest_path, serde_json::to_string_pretty(&value)?)
        .map_err(|e| DytesError::config(format!("写入 {} 失败: {e}", manifest_path.display())))?;

    println!(
        "已写入 {}（{}）: {}@{}；开始安装目标包",
        manifest_path.display(),
        key,
        name,
        version
    );

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| DytesError::failed(format!("创建异步运行时失败: {e}")))?;
    runtime
        .block_on(crate::update::dytes::install_one_dependency(
            dep_type,
            name,
            &version,
            !offline,
        ))
        .map_err(|e| DytesError::failed(format!("安装失败: {e:#}")))?;

    if json {
        println!(
            "{}",
            json!({
                "type": dep_type.as_str(),
                "name": name,
                "version": version,
                "dependency_key": key,
            })
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn upsert_dependency_adds_new_entry() {
        let mut manifest = json!({
            "name": "demo",
            "version": "0.1.0",
            "type": "app"
        });
        upsert_dependency(&mut manifest, "dependencies-core", "chatbot", "0.1.2").unwrap();
        assert_eq!(manifest["dependencies-core"]["chatbot"], "0.1.2");
        assert_eq!(manifest["dependencies-core"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn upsert_dependency_updates_existing_entry() {
        let mut manifest = json!({
            "dependencies-core": {
                "chatbot": "0.1.0",
                "cyctron": "1.0.18"
            }
        });
        upsert_dependency(&mut manifest, "dependencies-core", "chatbot", "0.1.2").unwrap();
        assert_eq!(manifest["dependencies-core"]["chatbot"], "0.1.2");
        assert_eq!(manifest["dependencies-core"]["cyctron"], "1.0.18");
    }

    #[test]
    fn upsert_dependency_rejects_non_object_group() {
        let mut manifest = json!({ "dependencies-core": null });
        assert!(upsert_dependency(&mut manifest, "dependencies-core", "chatbot", "0.1.2").is_err());
    }
}
