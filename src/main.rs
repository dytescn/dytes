use clap::{Args, Parser, Subcommand};
use dytes::install::service::package as install_package;
use dytes::shared::config::{DytesConfig, PackageType};
use dytes::shared::credentials::{mask, ToolConfig};
use dytes::shared::error::{DytesError, Result};
use dytes::shared::package;
use dytes::shared::platform;
use dytes::shared::templates;
use dytes::update::run::update_design::DesignKind;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
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
    /// 发布当前包
    Publish(PublishArgs),
    /// 初始化 dytes.json
    Init(InitArgs),
    /// 管理 S3 凭证与工具配置
    Config(ConfigArgs),
}

#[derive(Args)]
struct InstallArgs {
    /// 包类型：app/core/sys/web/plugin/design-soft/model
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
struct PublishArgs {
    #[arg(long)]
    dir: Option<PathBuf>,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    platform: Option<String>,
    #[arg(long)]
    force: bool,
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
        Commands::Install(args) => run_install(args),
        Commands::Update(args) => run_update(args),
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
    let manifest = templates::init_manifest(pkg_type, &name, &args.version);
    fs::write(&path, serde_json::to_string_pretty(&manifest)?)?;
    if args.json {
        println!(
            "{}",
            json!({ "path": path, "type": pkg_type.as_str(), "name": name, "version": args.version })
        );
    } else {
        println!("已生成 {}", path.display());
        println!("下一步：dytes publish --dry-run");
    }
    Ok(())
}

fn run_publish(args: PublishArgs) -> Result<()> {
    let dir = args.dir.unwrap_or(std::env::current_dir()?);
    let config_path = args.config.unwrap_or_else(|| dir.join("dytes.json"));
    let config = DytesConfig::read(&config_path)?;
    let pkg_type = config.package_type()?;
    let platform = args.platform.unwrap_or_else(platform::current_platform);
    let Some((exports, platform_tag)) = platform::select_exports(&config, &platform) else {
        return Err(DytesError::config(format!(
            "当前平台 {platform} 未在 platform-exports 中配置"
        )));
    };
    if exports.is_empty() {
        return Err(DytesError::config("没有可导出的文件"));
    }
    let prefix = format!("{}/{}", package::remote_prefix(pkg_type), config.name);
    let base = platform::remote_base(&prefix, &config.version, platform_tag);
    let mut plan = Vec::new();
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
            "exists": dir.join(source).exists(),
        }));
    }
    let index = package::packages_index(pkg_type);
    if args.json {
        println!(
            "{}",
            json!({
                "dry_run": args.dry_run,
                "type": pkg_type.as_str(),
                "name": config.name,
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
        }
    }
    if !args.dry_run {
        return Err(DytesError::failed(
            "publish 上传引擎尚未接入，请先使用 --dry-run 或按 todo.md D-07 完成迁移",
        ));
    }
    Ok(())
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
        other => {
            return Err(DytesError::failed(format!(
                "install 暂不支持类型 {other}；当前支持 plugin 与 design-soft"
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

fn run_update_inner(args: UpdateArgs) -> Result<()> {
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
        dytes::update::run::dytes::init_dytes_with_options(None, sync_online)
            .await
            .map_err(|e| DytesError::failed(format!("更新失败: {e:#}")))
    })
}
