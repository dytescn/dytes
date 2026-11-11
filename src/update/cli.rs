//! `dytes update` 与 `dytes self-update` 命令入口。

use clap::Args;
use std::path::PathBuf;

use crate::utils::error::{DytesError, Result};

#[derive(Args)]
pub struct UpdateArgs {
    #[arg(long)]
    all: bool,
    #[arg(long = "type", value_name = "TYPE[,TYPE...]")]
    pub(crate) pkg_types: Option<String>,
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
    /// 启动图形更新窗口
    #[arg(long)]
    gui: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
pub struct SelfUpdateArgs {
    /// 只检查是否有新版本，不下载
    #[arg(long)]
    check: bool,
    /// 同版本也重新下载
    #[arg(long)]
    force: bool,
    /// 下载来源：s3（默认；CDN 已停用，避免缓存导致更新失败）
    #[arg(long, value_name = "s3", default_value = "s3")]
    source: String,
    #[arg(long)]
    json: bool,
}

pub fn run(args: UpdateArgs) -> Result<()> {
    if args.gui {
        #[cfg(feature = "gui")]
        {
            return crate::utils::gui::run_update_gui(args.dir.clone())
                .map_err(|error| DytesError::failed(format!("{error:#}")));
        }
        #[cfg(not(feature = "gui"))]
        {
            return Err(DytesError::args(
                "当前构建未启用图形界面，请使用启用 `gui` feature 的构建",
            ));
        }
    }

    if let Some(dir) = &args.dir {
        std::env::set_current_dir(dir)
            .map_err(|e| DytesError::failed(format!("切换目录失败: {e}")))?;
    }

    // 更新链路包含多层 async future；给独立线程更大的栈，避免 Windows
    // 主线程 1 MiB 栈在 debug 构建下溢出。
    let handle = std::thread::Builder::new()
        .name("dytes-update".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || run_inner(args))
        .map_err(|e| DytesError::failed(format!("创建更新线程失败: {e}")))?;

    match handle.join() {
        Ok(result) => result,
        Err(_) => Err(DytesError::failed("更新线程异常退出")),
    }
}

pub fn run_self_update(args: SelfUpdateArgs) -> Result<()> {
    match args.source.trim().to_ascii_lowercase().as_str() {
        // `auto` 保留为兼容值：现在两者都只走 S3。
        "s3" | "auto" => {}
        "cdn" | "packages" => {
            return Err(DytesError::args(
                "--source cdn 已停用：self-update 只使用 S3，避免 CDN 缓存导致更新失败",
            ))
        }
        other => {
            return Err(DytesError::args(format!(
                "--source 只支持 s3（收到 {other}）"
            )))
        }
    }
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| DytesError::failed(format!("创建异步运行时失败: {e}")))?;
    runtime
        .block_on(crate::update::self_update::run(
            args.check, args.force, args.json,
        ))
        .map_err(|e| DytesError::failed(format!("自更新失败: {e:#}")))?;
    Ok(())
}

fn run_inner(args: UpdateArgs) -> Result<()> {
    if args.startup
        && !crate::update::dytes::startup_allowed()
            .map_err(|e| DytesError::failed(format!("检查启动更新状态失败: {e:#}")))?
    {
        return Ok(());
    }

    if args.startup {
        // 启动链路只允许快速失败：不重试，并对整个更新设置总超时，
        // 避免网络或凭证异常时阻塞宿主启动。
        crate::update::loader::set_startup_mode(true);
    }

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| DytesError::failed(format!("创建异步运行时失败: {e}")))?;
    let sync_online = !args.offline;
    runtime.block_on(async {
        if args.discover {
            return crate::update::dytes::discover_packages()
                .await
                .map_err(|e| DytesError::failed(format!("扫描失败: {e:#}")));
        }
        if args.init {
            return crate::update::dytes::run_init_scripts()
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
            let report = crate::update::dytes::check_report(sync_online, types.as_deref())
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

        let update = crate::update::dytes::init_dytes_with_options(None, sync_online);
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
            crate::update::dytes::mark_startup_checked();
        }
        result.map_err(|e| DytesError::failed(format!("更新失败: {e:#}")))
    })
}
