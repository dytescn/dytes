use super::{update_design, update_os, update_sys};
use crate::update::utils::dytes::DytesConfig;
use crate::update::utils::env::get_app_dir;
use crate::update::utils::loader::{download_file, fetch_text};
use crate::update::utils::lock::{get_package_version_from_lock, update_package_in_lock};
use crate::update::utils::version::check_version;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Web 资源更新结果，包含是否需要执行 init 脚本
pub struct WebUpdateResult {
    pub package_name: String,
    pub web_dir: String,
    pub init_scripts: Vec<String>,
}

/// 去除版本号前缀（^ / ~）
fn parse_version(version: &str) -> Option<String> {
    let cleaned = version.trim_start_matches('^').trim_start_matches('~');
    if cleaned.parse::<semver::Version>().is_ok() {
        Some(cleaned.to_string())
    } else {
        None
    }
}

pub async fn update_webs_info(
    name: String,
    declared_version: String,
) -> Result<Option<WebUpdateResult>> {
    println!("更新 Web 资源: {} (声明版本 {})", name, declared_version);

    // 1. 获取远程包信息（使用 S3 路径）
    let s3_path = format!("webs/{}/dytes.json", name);
    println!("  从 S3 获取: {}", s3_path);

    let remote_text = fetch_text(&s3_path)
        .await
        .with_context(|| format!("获取 webs/{}/dytes.json 失败", name))?;
    let remote_app_info: DytesConfig = serde_json::from_str(&remote_text)
        .with_context(|| format!("解析 webs/{}/dytes.json 失败", name))?;

    let platform = crate::update::utils::platform::current_platform();
    let Some((exports, platform_tag)) =
        crate::update::utils::platform::select_exports(&remote_app_info, &platform)
    else {
        anyhow::bail!("包 {} 未发布平台 {}", name, platform);
    };
    let base_path = crate::update::utils::platform::remote_base(
        &format!("webs/{}", name),
        &remote_app_info.version,
        platform_tag,
    );

    println!("  远程版本: {}", remote_app_info.version);
    println!("  平台: {}", platform);
    println!("  导出文件: {:?}", exports);

    // 先安装 Web 包声明的依赖（例如 init 依赖 database），再判断自身是否更新
    for (dep_name, dep_version) in &remote_app_info.dependencies_core {
        println!("  安装 Web 包核心依赖: {} {}", dep_name, dep_version);
        update_os::update_os_info(dep_name.clone(), dep_version.clone()).await?;
    }
    for (dep_name, dep_version) in &remote_app_info.dependencies_sys {
        println!("  安装 Web 包系统依赖: {} {}", dep_name, dep_version);
        update_sys::update_sys_info(dep_name.clone(), dep_version.clone()).await?;
    }
    for (dep_name, dep_version) in &remote_app_info.dependencies_design_plugins {
        println!("  安装 Web 包设计插件依赖: {} {}", dep_name, dep_version);
        update_design::update_design_info(
            dep_name.clone(),
            dep_version.clone(),
            update_design::DesignKind::Plugin,
            false,
            None,
        )
        .await?;
    }
    for (dep_name, dep_version) in &remote_app_info.dependencies_design_soft {
        println!("  安装 Web 包设计软件依赖: {} {}", dep_name, dep_version);
        update_design::update_design_info(
            dep_name.clone(),
            dep_version.clone(),
            update_design::DesignKind::Soft,
            false,
            None,
        )
        .await?;
    }

    // 声明版本可能滞后于远程实际版本，以远程版本为准判断是否真的需要更新
    if let Ok(Some(lock_version)) = get_package_version_from_lock(&name) {
        if let (Some(local), Some(remote)) = (
            parse_version(&lock_version),
            parse_version(&remote_app_info.version),
        ) {
            if !check_version(&local, &remote) {
                println!("  已是最新版本 (v{})，跳过下载", local);
                return Ok(None);
            }
        }
    }

    // 2. 获取应用目录 - web/{库名}/
    let app_dir = get_app_dir();
    let web_dir = format!("{}/web/{}", app_dir.trim_end_matches('/'), name);
    fs::create_dir_all(&web_dir)?;

    // 3. 下载导出的文件（使用 S3 路径）
    for dest_filename in exports.values() {
        let s3_path = format!("{}{}", base_path, dest_filename);
        let dest_path = format!("{}/{}", web_dir, dest_filename);

        println!("  下载: {} -> {}", s3_path, dest_path);

        if !download_file(&s3_path, &web_dir).await {
            anyhow::bail!("下载 Web 资源 {} 失败", dest_filename);
        }

        println!("  下载完成: {}", dest_filename);
    }

    // 4. 如果有 init 脚本，也下载（使用 S3 路径）
    for init_script in &remote_app_info.init {
        let filename = Path::new(init_script)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        let s3_path = format!("{}{}", base_path, filename);
        let dest_path = format!("{}/{}", web_dir, filename);

        println!("  下载初始化脚本: {} -> {}", s3_path, dest_path);

        if !download_file(&s3_path, &web_dir).await {
            anyhow::bail!("下载初始化脚本 {} 失败", init_script);
        }
    }

    // 5. 构建 loader 数据
    let mut loader = Vec::new();
    for (source, dest) in exports {
        loader.push(vec![source.clone(), dest.clone()]);
    }

    // 6. 写入锁文件
    update_package_in_lock(
        remote_app_info.uuid.as_deref(),
        &name,
        &remote_app_info.version,
        "web",
        loader,
    )?;

    println!("  Web 资源下载完成: {} v{}", name, remote_app_info.version);

    // 7. 返回需要执行 init 脚本的信息
    if !remote_app_info.init.is_empty() {
        Ok(Some(WebUpdateResult {
            package_name: name,
            web_dir,
            init_scripts: remote_app_info.init,
        }))
    } else {
        Ok(None)
    }
}
