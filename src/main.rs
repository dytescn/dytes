//! dytes 命令行入口：命令树定义与分派。

use clap::{Parser, Subcommand};

use dytes::init::InitArgs;
use dytes::install::cli::InstallArgs;
use dytes::publish::cli::{CheckArgs, PublishArgs};
use dytes::update::cli::{SelfUpdateArgs, UpdateArgs};
use dytes::utils::config::ConfigArgs;
use dytes::utils::error::Result;

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

fn main() {
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
        Commands::Publish(args) => dytes::publish::cli::run(args),
        Commands::Check(args) => dytes::publish::cli::run_check(args),
        Commands::Init(args) => dytes::init::run(args),
        Commands::Config(args) => dytes::utils::config::run(args),
    }
}
