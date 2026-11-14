//! `dytes publish` 与 `dytes check` 命令入口。

use clap::Args;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::checksum::digest_file;
use crate::utils::dytes::{DytesConfig, PackageType};
use crate::utils::error::{DytesError, Result};
use crate::utils::platform;
use crate::utils::plugin_manifest::{self, CheckReport};
use crate::utils::package;

#[derive(Args)]
pub struct PublishArgs {
    #[arg(long)]
    dir: Option<PathBuf>,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    platform: Option<String>,
    /// 只发布 dytes.json，不上传产物（需求 / 训练类包）
    #[arg(long = "manifest-only")]
    manifest_only: bool,
    /// 跳过设计插件清单校验（不建议）
    #[arg(long = "skip-check")]
    skip_check: bool,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
pub struct CheckArgs {
    /// 包目录（默认当前目录）
    #[arg(value_name = "DIR")]
    dir: Option<PathBuf>,
    /// 只接受指定类型；与清单 type 不一致时报错
    #[arg(long = "type", value_name = "TYPE")]
    pkg_type: Option<String>,
    /// 按指定平台选择 platform-exports（默认当前平台）
    #[arg(long)]
    platform: Option<String>,
    /// 允许没有 exports 的“仅清单”包
    #[arg(long = "manifest-only")]
    manifest_only: bool,
    #[arg(long)]
    json: bool,
}

pub fn run(args: PublishArgs) -> Result<()> {
    let dir = args.dir.unwrap_or(std::env::current_dir()?);
    let config_path = args.config.unwrap_or_else(|| dir.join("dytes.json"));
    let config = DytesConfig::read_with(&config_path, args.manifest_only)?;
    let pkg_type = config.package_type()?;
    let platform = args
        .platform
        .clone()
        .unwrap_or_else(platform::current_platform);

    // 设计插件在发布前执行完整清单校验；`--skip-check` 仅供紧急发布使用。
    if pkg_type == PackageType::DesignPlugins && !args.skip_check {
        let manifest: serde_json::Value = serde_json::from_str(&fs::read_to_string(&config_path)?)
            .map_err(|e| DytesError::config(format!("解析 {} 失败: {e}", config_path.display())))?;
        let package_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        plugin_manifest::ensure_design_plugin(package_dir, &manifest, &platform)?;
    }

    let uuid = config.uuid.as_deref().unwrap_or("");
    let mut plan = Vec::new();
    if !args.manifest_only {
        let Some((exports, platform_tag)) = platform::select_exports(&config, &platform) else {
            return Err(DytesError::config(format!(
                "当前平台 {platform} 未在 platform-exports 中配置"
            )));
        };
        if exports.is_empty() {
            return Err(DytesError::config(
                "没有可导出的文件；需求 / 训练类包请使用 --manifest-only",
            ));
        }
        let prefix = format!("{}/{}", package::remote_prefix(pkg_type), config.name);
        let base = platform::remote_base(&prefix, &config.version, platform_tag);
        for (source, dest) in exports {
            if dest.starts_with('/')
                || dest.starts_with('\\')
                || dest.split(['/', '\\']).any(|part| part == "..")
            {
                return Err(DytesError::config(format!("不支持的导出路径: {dest}")));
            }
            plan.push(json!({
                "source": source,
                "remote": format!("{base}{dest}"),
                "exists": dir.join(source.as_str()).exists(),
                "checksum": if dir.join(source.as_str()).exists() {
                    Some(digest_file(&dir.join(source.as_str())).map_err(|error| {
                        DytesError::failed(format!("计算校验失败: {error:#}"))
                    })?)
                } else {
                    None
                },
            }));
        }
    }
    let index = package::packages_index(pkg_type);
    if args.json {
        println!(
            "{}",
            json!({
                "dry_run": args.dry_run,
                "manifest_only": args.manifest_only,
                "type": pkg_type.as_str(),
                "name": config.name,
                "uuid": uuid,
                "version": config.version,
                "platform": platform,
                "index": index,
                "files": plan,
            })
        );
    } else {
        println!(
            "包: {} {} ({})",
            config.name,
            config.version,
            pkg_type.as_str()
        );
        println!(
            "UUID: {}",
            if uuid.is_empty() {
                "<publish 时生成>"
            } else {
                uuid
            }
        );
        println!("平台: {platform}");
        println!("索引: {index}");
        for item in &plan {
            println!(
                "  {} -> {}{}",
                item["source"].as_str().unwrap_or(""),
                item["remote"].as_str().unwrap_or(""),
                if item["exists"].as_bool().unwrap_or(false) {
                    ""
                } else {
                    "  (缺失)"
                }
            );
            if let Some(sha256) = item["checksum"]["sha256"].as_str() {
                println!(
                    "    sha256={} sampled={}",
                    sha256,
                    item["checksum"]["sampled"].as_bool().unwrap_or(false)
                );
            }
        }
    }
    if args.dry_run {
        let missing: Vec<&str> = plan
            .iter()
            .filter(|item| {
                !item
                    .get("exists")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
            .filter_map(|item| item.get("source").and_then(|v| v.as_str()))
            .collect();
        if !missing.is_empty() {
            return Err(DytesError::config(format!(
                "导出文件缺失: {}",
                missing.join(", ")
            )));
        }
        return Ok(());
    }
    crate::publish::dytes::publish_with_options(
        &config_path,
        args.force,
        args.manifest_only,
        args.skip_check,
    )
    .map_err(|error| DytesError::failed(format!("{error:#}")))
}

/// 一次清单校验的完整结果，供 `check` 与 `test` 复用。
pub struct CheckOutcome {
    pub config: DytesConfig,
    pub manifest_text: String,
    pub report: CheckReport,
    pub platform: String,
}

/// 只做校验、不打印，返回清单、配置与逐项结果。
///
/// 类型不匹配、清单无法读取等前置错误直接返回 `Err`；校验项失败则体现在
/// `report.items` 中，由调用方决定如何输出。
pub fn evaluate_check(
    dir: &Path,
    expected_type: Option<&str>,
    platform: &str,
    manifest_only: bool,
) -> Result<CheckOutcome> {
    let config_path = dir.join("dytes.json");
    let text = fs::read_to_string(&config_path)
        .map_err(|e| DytesError::config(format!("读取 {} 失败: {e}", config_path.display())))?;
    let manifest: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| DytesError::config(format!("解析 {} 失败: {e}", config_path.display())))?;
    let config = DytesConfig::read_with(&config_path, manifest_only)?;
    let pkg_type = config.package_type()?;

    if let Some(expected) = expected_type {
        let expected = PackageType::parse(expected)
            .ok_or_else(|| DytesError::args(format!("未知包类型: {expected}")))?;
        if expected != pkg_type {
            return Err(DytesError::config(format!(
                "清单类型为 {}，与 --type {} 不一致",
                pkg_type.as_str(),
                expected.as_str()
            )));
        }
    }

    let mut report = CheckReport::default();
    report.pass("dytes.json 可解析", config_path.display().to_string());

    match pkg_type {
        PackageType::DesignPlugins => {
            report.extend(plugin_manifest::check_design_plugin(
                dir, &manifest, platform,
            ));
        }
        _ if manifest_only => {}
        _ => match platform::select_exports(&config, platform) {
            Some((exports, _)) if !exports.is_empty() => {
                report.pass("exports 非空", format!("{} 个", exports.len()));
                for source in exports.keys() {
                    let path = dir.join(source.trim_start_matches("./"));
                    report.record(
                        format!("exports 文件存在: {source}"),
                        path.is_file(),
                        path.display().to_string(),
                    );
                }
            }
            Some(_) => report.fail("exports 非空", "<缺失>"),
            None => report.fail("当前平台导出", format!("platform-exports 缺少 {platform}")),
        },
    }

    Ok(CheckOutcome {
        config,
        manifest_text: text,
        report,
        platform: platform.to_string(),
    })
}

pub fn run_check(args: CheckArgs) -> Result<()> {
    let dir = args.dir.unwrap_or(std::env::current_dir()?);
    let platform = args.platform.unwrap_or_else(platform::current_platform);
    let outcome = evaluate_check(
        &dir,
        args.pkg_type.as_deref(),
        &platform,
        args.manifest_only,
    )?;
    let report = outcome.report;

    if args.json {
        let checks: Vec<serde_json::Value> = report
            .items
            .iter()
            .map(|item| json!({ "label": item.label, "ok": item.ok, "detail": item.detail }))
            .collect();
        println!("{}", json!({ "passed": report.passed(), "checks": checks }));
    } else {
        report.print();
    }

    if report.passed() {
        Ok(())
    } else {
        Err(DytesError::config(format!(
            "清单校验失败: {} 项",
            report.failure_count()
        )))
    }
}
