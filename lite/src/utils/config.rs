//! `dytes config` 命令入口。

use clap::{Args, Subcommand};
use serde_json::json;

use crate::utils::credentials::{mask, ToolConfig};
use crate::utils::error::{DytesError, Result};

#[derive(Args)]
pub struct ConfigArgs {
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

pub fn run(args: ConfigArgs) -> Result<()> {
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
