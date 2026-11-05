use clap::{Args, Parser, Subcommand};
use dytes::install::service::package as install_package;
use dytes::shared::checksum::digest_file;
use dytes::shared::config::{DytesConfig, PackageType};
use dytes::shared::credentials::{mask, ToolConfig};
use dytes::shared::error::{DytesError, Result};
use dytes::shared::package;
use dytes::shared::platform;
use dytes::shared::plugin_manifest::{self, CheckReport};
use dytes::shared::templates;
use dytes::update::run::update_design::DesignKind;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Parser)]
#[command(
    name = "dytes",
    version,
    about = "Piksel 包安装、更新、发布与初始化工具"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 安装应用、插件或依赖包
    Install(InstallArgs),
    /// 检查并安装依赖更新
    Update(UpdateArgs),
    /// 更新 dytes CLI 自身
    SelfUpdate(SelfUpdateArgs),
    /// 发布当前包
    Publish(PublishArgs),
    /// 校验包清单（设计插件会执行完整规则）
    Check(CheckArgs),
    /// 初始化 dytes.json
    Init(InitArgs),
    /// 管理 S3 凭证与工具配置
    Config(ConfigArgs),
}

#[derive(Args)]
struct InstallArgs {
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

#[derive(Args)]
struct UpdateArgs {
    #[arg(long)]
    all: bool,
    #[arg(long = "type", value_name = "TYPE[,TYPE...]")]
    pkg_types: Option<String>,
    #[arg(long)]
    check: bool,
    #[arg(long, alias = "no-online")]
    offline: bool,
    #[arg(long)]
    discover: bool,
    #[arg(long)]
    init: bool,
    #[arg(long)]
    startup: bool,
    #[arg(long, alias = "install")]
    auto: bool,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    dir: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct SelfUpdateArgs {
    /// 只检查是否有新版本，不下载
    #[arg(long)]
    check: bool,
    /// 同版本也重新下载
    #[arg(long)]
    force: bool,
    /// 下载来源：auto（S3 优先，CDN 回退）/ cdn / s3
    #[arg(long, value_name = "auto|cdn|s3", default_value = "auto")]
    source: String,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct PublishArgs {
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
struct CheckArgs {
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

#[derive(Args)]
struct InitArgs {
    #[arg(long = "type", default_value = "app")]
    pkg_type: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, default_value = "0.1.0")]
    version: String,
    #[arg(long)]
    dir: Option<PathBuf>,
    /// 生成不含 exports / platform-exports 的最小清单
    #[arg(long = "no-exports")]
    no_exports: bool,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// 列出当前配置
    List {
        #[arg(long)]
        json: bool,
    },
    /// 读取配置项
    Get {
        key: String,
        #[arg(long)]
        reveal: bool,
        #[arg(long)]
        json: bool,
    },
    /// 设置配置项（敏感值省略时交互输入）
    Set {
        key: String,
        value: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// 删除配置项
    Unset { key: String },
    /// 输出配置文件路径
    Path {
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{}", error.message);
            process::exit(error.code);
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Config(args) => run_config(args),
        Commands::Init(args) => run_init(args),
        Commands::Publish(args) => run_publish(args),
        Commands::Check(args) => run_check(args),
        Commands::Install(args) => run_install(args),
        Commands::Update(args) => run_update(args),
        Commands::SelfUpdate(args) => run_self_update(args),
    }
}

fn run_config(args: ConfigArgs) -> Result<()> {
    match args.command {
        ConfigCommand::List { json } => {
            let config = ToolConfig::load()?;
            let resolved = config.resolve();
            if json {
                println!(
                    "{}",
                    json!({
                        "path": ToolConfig::path(),
                        "s3": {
                            "endpoint": resolved.s3.endpoint,
                            "bucket": resolved.s3.bucket,
                            "region": resolved.s3.region,
                            "access_key": mask(resolved.s3.access_key.as_deref()),
                            "secret_key": mask(resolved.s3.secret_key.as_deref()),
                        },
                        "platform": resolved.platform,
                    })
                );
            } else {
                println!("配置文件: {}", ToolConfig::path().display());
                println!("s3.endpoint   = {}", resolved.s3.endpoint);
                println!("s3.bucket     = {}", resolved.s3.bucket);
                println!("s3.region     = {}", resolved.s3.region);
                println!(
                    "s3.access_key = {}",
                    mask(resolved.s3.access_key.as_deref())
                );
                println!(
                    "s3.secret_key = {}",
                    mask(resolved.s3.secret_key.as_deref())
                );
                println!("platform      = {}", resolved.platform);
            }
            Ok(())
        }
        ConfigCommand::Get { key, reveal, json } => {
            let config = ToolConfig::load()?;
            let resolved = config.resolve();
            let value = match key.as_str() {
                "s3.endpoint" => Some(resolved.s3.endpoint),
                "s3.bucket" => Some(resolved.s3.bucket),
                "s3.region" => Some(resolved.s3.region),
                "s3.access_key" => resolved.s3.access_key,
                "s3.secret_key" => resolved.s3.secret_key,
                "platform" => Some(resolved.platform),
                _ => return Err(DytesError::args(format!("未知配置键: {key}"))),
            };
            let sensitive = matches!(key.as_str(), "s3.access_key" | "s3.secret_key");
            let display = if sensitive && !reveal {
                mask(value.as_deref())
            } else {
                value.clone().unwrap_or_else(|| "(未设置)".to_string())
            };
            if json {
                println!("{}", json!({ "key": key, "value": display }));
            } else {
                println!("{display}");
            }
            let _ = value;
            Ok(())
        }
        ConfigCommand::Set { key, value, json } => {
            let mut config = ToolConfig::load()?;
            let value = match value {
                Some(value) => value,
                None if key == "s3.secret_key" || key == "s3.access_key" => {
                    rpassword::prompt_password(format!("{key}: "))
                        .map_err(|e| DytesError::failed(format!("读取输入失败: {e}")))?
                }
                None => return Err(DytesError::args("缺少 value")),
            };
            config.set_key(&key, &value)?;
            config.save()?;
            if json {
                println!("{}", json!({ "key": key, "saved": true }));
            } else if key == "s3.secret_key" || key == "s3.access_key" {
                println!("已保存 {key} -> ****");
            } else {
                println!("已保存 {key} -> {value}");
            }
            Ok(())
        }
        ConfigCommand::Unset { key } => {
            let mut config = ToolConfig::load()?;
            config.unset_key(&key)?;
            config.save()?;
            println!("已删除 {key}");
            Ok(())
        }
        ConfigCommand::Path { json } => {
            let path = ToolConfig::path();
            if json {
                println!("{}", json!({ "path": path }));
            } else {
                println!("{}", path.display());
            }
            Ok(())
        }
    }
}

fn run_check(args: CheckArgs) -> Result<()> {
    let dir = args.dir.unwrap_or(std::env::current_dir()?);
    let config_path = dir.join("dytes.json");
    let text = fs::read_to_string(&config_path)
        .map_err(|e| DytesError::config(format!("读取 {} 失败: {e}", config_path.display())))?;
    let manifest: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| DytesError::config(format!("解析 {} 失败: {e}", config_path.display())))?;
    let config = DytesConfig::read_with(&config_path, args.manifest_only)?;
    let pkg_type = config.package_type()?;

    if let Some(expected) = args.pkg_type.as_deref() {
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

    let platform = args.platform.unwrap_or_else(platform::current_platform);
    let mut report = CheckReport::default();
    report.pass("dytes.json 可解析", config_path.display().to_string());

    match pkg_type {
        PackageType::DesignPlugins => {
            report.extend(plugin_manifest::check_design_plugin(
                &dir, &manifest, &platform,
            ));
        }
        _ if args.manifest_only => {}
        _ => match platform::select_exports(&config, &platform) {
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

    if args.json {
        let checks: Vec<serde_json::Value> = report
            .items
            .iter()
            .map(|item| {
                json!({ "label": item.label, "ok": item.ok, "detail": item.detail })
            })
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

fn run_init(args: InitArgs) -> Result<()> {
    let pkg_type = PackageType::parse(&args.pkg_type)
        .ok_or_else(|| DytesError::args(format!("未知包类型: {}", args.pkg_type)))?;
    let dir = args.dir.unwrap_or(std::env::current_dir()?);
    let name = args
        .name
        .or_else(|| {
            dir.file_name()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .ok_or_else(|| DytesError::args("无法从目录名推断包名，请使用 --name"))?;
    let path = dir.join("dytes.json");
    if path.exists() && !args.force {
        return Err(DytesError::config(format!(
            "{} 已存在，使用 --force 覆盖",
            path.display()
        )));
    }
    fs::create_dir_all(&dir)?;
    let manifest = templates::init_manifest(pkg_type, &name, &args.version, !args.no_exports);
    let uuid = manifest
        .get("uuid")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    fs::write(&path, serde_json::to_string_pretty(&manifest)?)?;
    if args.json {
        println!(
            "{}",
            json!({ "path": path, "type": pkg_type.as_str(), "name": name, "uuid": uuid, "version": args.version })
        );
    } else {
        println!("已生成 {}", path.display());
        println!("包 UUID: {}", uuid);
        println!("下一步：dytes publish --dry-run");
    }
    Ok(())
}

fn run_publish(args: PublishArgs) -> Result<()> {
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
            .filter(|item| !item.get("exists").and_then(|v| v.as_bool()).unwrap_or(false))
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
    dytes::publish::run::dytes::publish_with_options(
        &config_path,
        args.force,
        args.manifest_only,
        args.skip_check,
    )
    .map_err(|error| DytesError::failed(format!("{error:#}")))
}

fn run_install(args: InstallArgs) -> Result<()> {
    let dir = args.dir.clone().unwrap_or(std::env::current_dir()?);
    std::env::set_current_dir(&dir)
        .map_err(|e| DytesError::failed(format!("切换目录失败: {e}")))?;

    let pkg_type = args.pkg_type.as_deref().ok_or_else(|| {
        DytesError::args("请指定包类型，例如：dytes install plugin cdr_check_txt")
    })?;
    let (name, version) = split_name_version(args.name.as_deref(), args.version.as_deref())?;

    let kind = match pkg_type.trim().to_ascii_lowercase().as_str() {
        "plugin" | "plugins" | "design_plugins" | "design-plugins" => DesignKind::Plugin,
        "design-soft" | "design_soft" | "designsoft" => DesignKind::Soft,
        "sys" | "system" | "core" | "web" | "webs" | "model" | "models" => {
            return install_dependency(
                &dir,
                pkg_type,
                &name,
                version.as_deref(),
                args.offline,
                args.json,
            )
        }
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

/// `dependencies-*` 键名。
fn dependency_key(pkg_type: &str) -> Option<&'static str> {
    match pkg_type.trim().to_ascii_lowercase().as_str() {
        "core" => Some("dependencies-core"),
        "sys" | "system" => Some("dependencies-sys"),
        "web" | "webs" => Some("dependencies-web"),
        "model" | "models" => Some("dependencies-model"),
        _ => None,
    }
}

/// 远程清单目录前缀。
fn dependency_remote_prefix(pkg_type: &str) -> Option<&'static str> {
    match pkg_type.trim().to_ascii_lowercase().as_str() {
        "core" => Some("core"),
        "sys" | "system" => Some("system"),
        "web" | "webs" => Some("webs"),
        "model" | "models" => Some("models"),
        _ => None,
    }
}

async fn fetch_dependency_version(pkg_type: &str, name: &str) -> anyhow::Result<String> {
    let prefix = dependency_remote_prefix(pkg_type)
        .ok_or_else(|| anyhow::anyhow!("未知包类型: {pkg_type}"))?;
    let path = format!("{}/{}/dytes.json", prefix, name);
    let text = dytes::update::utils::loader::fetch_text(&path).await?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("解析 {path} 失败: {e}"))?;
    value
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("{path} 缺少 version"))
}

/// 安装 sys / core / web / model 依赖：写入当前清单后执行 update。
fn install_dependency(
    dir: &std::path::Path,
    pkg_type: &str,
    name: &str,
    version: Option<&str>,
    offline: bool,
    json: bool,
) -> Result<()> {
    let key = dependency_key(pkg_type)
        .ok_or_else(|| DytesError::args(format!("install 暂不支持类型 {pkg_type}")))?;
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
    let object = value
        .as_object_mut()
        .ok_or_else(|| DytesError::config("dytes.json 不是 JSON 对象"))?;

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
                .block_on(fetch_dependency_version(pkg_type, name))
                .map_err(|e| DytesError::failed(format!("获取线上版本失败: {e:#}")))?
        }
    };

    let entry = object
        .entry(key.to_string())
        .or_insert_with(|| serde_json::json!({}));
    let deps = entry
        .as_object_mut()
        .ok_or_else(|| DytesError::config(format!("{key} 不是对象")))?;
    deps.insert(name.to_string(), serde_json::Value::String(version.clone()));
    fs::write(&manifest_path, serde_json::to_string_pretty(&value)?)
        .map_err(|e| DytesError::config(format!("写入 {} 失败: {e}", manifest_path.display())))?;

    println!("已写入 {}: {}@{}；开始更新依赖", key, name, version);

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| DytesError::failed(format!("创建异步运行时失败: {e}")))?;
    runtime
        .block_on(dytes::update::run::dytes::init_dytes_with_options(
            None, !offline,
        ))
        .map_err(|e| DytesError::failed(format!("更新失败: {e:#}")))?;

    if json {
        println!(
            "{}",
            json!({
                "type": pkg_type,
                "name": name,
                "version": version,
                "dependency_key": key,
            })
        );
    }
    Ok(())
}

/// 解析 `name` / `name@version`，并合并 `--version` 参数。
fn split_name_version(
    name: Option<&str>,
    version: Option<&str>,
) -> Result<(String, Option<String>)> {
    let raw = name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DytesError::args("缺少包名"))?;

    let (name, inline_version) = match raw.split_once('@') {
        Some((name, version)) if !name.is_empty() && !version.is_empty() => {
            (name.to_string(), Some(version.to_string()))
        }
        _ => (raw.to_string(), None),
    };

    Ok((name, version.map(str::to_string).or(inline_version)))
}

fn run_update(args: UpdateArgs) -> Result<()> {
    if let Some(dir) = &args.dir {
        std::env::set_current_dir(dir)
            .map_err(|e| DytesError::failed(format!("切换目录失败: {e}")))?;
    }

    // 更新链路包含多层 async future；给独立线程更大的栈，避免 Windows
    // 主线程 1 MiB 栈在 debug 构建下溢出。
    let handle = std::thread::Builder::new()
        .name("dytes-update".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || run_update_inner(args))
        .map_err(|e| DytesError::failed(format!("创建更新线程失败: {e}")))?;

    match handle.join() {
        Ok(result) => result,
        Err(_) => Err(DytesError::failed("更新线程异常退出")),
    }
}

fn run_self_update(args: SelfUpdateArgs) -> Result<()> {
    let source = dytes::update::run::self_update::UpdateSource::parse(&args.source)
        .ok_or_else(|| DytesError::args("--source 只支持 auto / cdn / s3"))?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| DytesError::failed(format!("创建异步运行时失败: {e}")))?;
    runtime
        .block_on(dytes::update::run::self_update::run(
            args.check, args.force, args.json, source,
        ))
        .map_err(|e| DytesError::failed(format!("自更新失败: {e:#}")))?;
    Ok(())
}

fn run_update_inner(args: UpdateArgs) -> Result<()> {
    if args.startup
        && !dytes::update::run::dytes::startup_allowed()
            .map_err(|e| DytesError::failed(format!("检查启动更新状态失败: {e:#}")))?
    {
        return Ok(());
    }

    if args.startup {
        // 启动链路只允许快速失败：不重试，并对整个更新设置总超时，
        // 避免网络或凭证异常时阻塞宿主启动。
        dytes::update::utils::loader::set_startup_mode(true);
    }

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| DytesError::failed(format!("创建异步运行时失败: {e}")))?;
    let sync_online = !args.offline;
    runtime.block_on(async {
        if args.discover {
            return dytes::update::run::dytes::discover_packages()
                .await
                .map_err(|e| DytesError::failed(format!("扫描失败: {e:#}")));
        }
        if args.init {
            return dytes::update::run::dytes::run_init_scripts()
                .await
                .map_err(|e| DytesError::failed(format!("执行 init 脚本失败: {e:#}")));
        }
        if args.check {
            let types: Option<Vec<String>> = args.pkg_types.as_ref().map(|value| {
                value
                    .split(',')
                    .map(|item| item.trim().to_string())
                    .filter(|item| !item.is_empty())
                    .collect()
            });
            let report = dytes::update::run::dytes::check_report(sync_online, types.as_deref())
                .await
                .map_err(|e| DytesError::failed(format!("检查更新失败: {e:#}")))?;
            if args.json {
                println!("{}", report.value);
            }
            if report.has_updates {
                return Err(DytesError::new(4, "有可用更新"));
            }
            println!("所有依赖已是最新版本");
            return Ok(());
        }

        let update = dytes::update::run::dytes::init_dytes_with_options(None, sync_online);
        let result = if args.startup {
            const STARTUP_TIMEOUT_SECS: u64 = 8;
            match tokio::time::timeout(std::time::Duration::from_secs(STARTUP_TIMEOUT_SECS), update)
                .await
            {
                Ok(result) => result,
                Err(_) => {
                    println!("启动更新超时（{STARTUP_TIMEOUT_SECS}s），本次跳过");
                    return Ok(());
                }
            }
        } else {
            update.await
        };
        if args.startup && result.is_ok() {
            dytes::update::run::dytes::mark_startup_checked();
        }
        result.map_err(|e| DytesError::failed(format!("更新失败: {e:#}")))
    })
}
