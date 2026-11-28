//! `dytes init` 命令入口。

use clap::Args;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

use crate::utils::dytes::PackageType;
use crate::utils::error::{DytesError, Result};
use crate::utils::templates;

#[derive(Args)]
pub struct InitArgs {
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

pub fn run(args: InitArgs) -> Result<()> {
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
        println!("下一步:dytes publish --dry-run");
    }
    Ok(())
}
