use crate::update::utils::dytes::DytesConfig;
use crate::update::utils::env::get_app_dir;
use crate::update::utils::loader::{download_file, fetch_text};
use crate::update::utils::lock::{get_package_version_from_lock, update_package_in_lock};
use crate::update::utils::version::check_version;
use anyhow::{Context, Result};
use std::fs;

/// 去除版本号前缀（^ / ~）
fn parse_version(version: &str) -> Option<String> {
    let cleaned = version.trim_start_matches('^').trim_start_matches('~');
    if cleaned.parse::<semver::Version>().is_ok() {
        Some(cleaned.to_string())
    } else {
        None
    }
}

pub async fn update_os_info(name: String, declared_version: String) -> Result<()> {
    println!("更新核心包: {} (声明版本 {})", name, declared_version);

    // 1. 获取远程包信息（使用 S3 路径）
    let s3_path = format!("core/{}/dytes.json", name);
    println!("  从 S3 获取: {}", s3_path);

    let remote_text = fetch_text(&s3_path)
        .await
        .with_context(|| format!("获取 {}/dytes.json 失败", name))?;
    let remote_app_info: DytesConfig = serde_json::from_str(&remote_text)
        .with_context(|| format!("解析 {}/dytes.json 失败", name))?;

    let platform = crate::update::utils::platform::current_platform();
    let Some((exports, platform_tag)) =
        crate::update::utils::platform::select_exports(&remote_app_info, &platform)
    else {
        anyhow::bail!("包 {} 未发布平台 {}", name, platform);
    };
    let base_path = crate::update::utils::platform::remote_base(
        &format!("core/{}", name),
        &remote_app_info.version,
        platform_tag,
    );

    println!("  远程版本: {}", remote_app_info.version);
    println!("  平台: {}", platform);
    println!("  导出文件: {:?}", exports);

    // 声明版本可能滞后于远程实际版本，以远程版本为准判断是否真的需要更新
    if let Ok(Some(lock_version)) = get_package_version_from_lock(&name) {
        if let (Some(local), Some(remote)) = (
            parse_version(&lock_version),
            parse_version(&remote_app_info.version),
        ) {
            if !check_version(&local, &remote) {
                println!("  已是最新版本 (v{})，跳过下载", local);
                return Ok(());
            }
        }
    }

    // 运行中的 update.exe 无法覆盖自身：下载为 update.new.exe，由 Piksel 启动时替换
    if name == "update" {
        let app_dir = get_app_dir();
        let tmp_dir = format!("{}/.update_tmp", app_dir.trim_end_matches('/'));
        fs::create_dir_all(&tmp_dir)?;

        let mut downloaded_file: Option<String> = None;
        for dest_filename in exports.values() {
            let s3_path = format!("{}{}", base_path, dest_filename);
            println!("  下载: {} -> {}/{}", s3_path, tmp_dir, dest_filename);
            if !download_file(&s3_path, &tmp_dir).await {
                anyhow::bail!("下载 {} 失败", dest_filename);
            }
            downloaded_file = Some(format!("{}/{}", tmp_dir, dest_filename));
        }

        if let Some(src) = downloaded_file {
            let dst = format!("{}/update.new.exe", app_dir.trim_end_matches('/'));
            let _ = fs::remove_file(&dst);
            fs::rename(&src, &dst)?;
            println!("  已准备新版本: {}（Piksel 启动时替换 update.exe）", dst);

            let mut loader = Vec::new();
            for (source, dest) in exports {
                loader.push(vec![source.clone(), dest.clone()]);
            }
            update_package_in_lock(
                remote_app_info.uuid.as_deref(),
                &name,
                &remote_app_info.version,
                "core",
                loader,
            )?;
            let _ = fs::remove_dir_all(&tmp_dir);
            println!("  更新包已准备: update v{}", remote_app_info.version);
        }
        return Ok(());
    }

    // 2. 获取应用目录 - 直接放到 ./
    let app_dir = get_app_dir();
    fs::create_dir_all(&app_dir)?;

    // 3. 下载导出的文件（使用 S3 路径）
    for dest_filename in exports.values() {
        let s3_path = format!("{}{}", base_path, dest_filename);

        // 下载到 ./{文件名}，确保路径正确
        let dest_path = format!("{}/{}", app_dir.trim_end_matches('/'), dest_filename);

        println!("  下载: {} -> {}", s3_path, dest_path);

        // 下载文件到 app_dir
        if !download_file(&s3_path, &app_dir).await {
            anyhow::bail!("下载 {} 失败", dest_filename);
        }

        println!("  下载完成: {}", dest_filename);
    }

    // 4. 构建 loader 数据
    let mut loader = Vec::new();
    for (source, dest) in exports {
        loader.push(vec![source.clone(), dest.clone()]);
    }

    // 5. 写入锁文件 - 使用 utils::lock
    update_package_in_lock(
        remote_app_info.uuid.as_deref(),
        &name,
        &remote_app_info.version,
        "core",
        loader,
    )?;

    println!("  更新完成: {} v{}", name, remote_app_info.version);
    Ok(())
}
