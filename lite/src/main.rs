//! dytes 命令行入口：命令树定义与分派。

use clap::{Parser, Subcommand};

use dytes::init::InitArgs;
use dytes::install::cli::InstallArgs;
use dytes::setup::SetupArgs;
use dytes::update::cli::{SelfUpdateArgs, UpdateArgs};
use dytes::utils::config::ConfigArgs;
use dytes::utils::error::Result;

#[derive(Parser)]
#[command(
    name = "dytes",
    version,
    about = "Design software package management tool"
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
    /// 初始化 dytes.json
    Init(InitArgs),
    /// 管理 S3 凭证与工具配置
    Config(ConfigArgs),
    /// 安装 dytes 自身（Windows 双击安装，写入用户 PATH）
    Setup(SetupArgs),
}

fn main() {
    // Windows 双击 dytes-windows.exe（无参数、父进程 explorer.exe）时进入安装模式；
    // 终端中带子命令调用的行为不变。
    if dytes::setup::should_run_installer() {
        std::process::exit(dytes::setup::run_double_click());
    }

    let cli = Cli::parse();
    if let Err(error) = run(cli) {
        eprintln!("{}", error.message);
        std::process::exit(error.code);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Install(args) => dytes::install::cli::run(args),
        Commands::Update(args) => dytes::update::cli::run(args),
        Commands::SelfUpdate(args) => dytes::update::cli::run_self_update(args),
        Commands::Init(args) => dytes::init::run(args),
        Commands::Config(args) => dytes::utils::config::run(args),
        Commands::Setup(args) => dytes::setup::run_cli(args),
    }
}
