use std::collections::HashMap;
use anyhow::{Result, anyhow, Context};
use serde::{Deserialize, Serialize};
use crate::utils::version::check_version;
use crate::utils::loader::download_file;
use crate::utils::env::get_app_dir;
use crate::utils::lock::{update_package_in_lock, get_package_version_from_lock};
use crate::utils::dytes::DytesConfig;
use semver::Version;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoteInfo {
    pub packages: HashMap<String, String>,
}

/// 解析版本号，去除 ^ 或 ~ 前缀
fn parse_version(version: &str) -> Option<String> {
    let cleaned = version.trim_start_matches('^').trim_start_matches('~');
    if cleaned.parse::<Version>().is_ok() {
        Some(cleaned.to_string())
    } else {
        None
    }
}

/// 判断是否为 HTML 文件
fn is_html_file(filename: &str) -> bool {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    matches!(ext.as_deref(), Some("html") | Some("htm"))
}

/// 获取文件名（不含路径）
fn get_filename_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

/// 更新应用本身（基于锁文件版本与远程版本比较）
pub async fn update_app_info(name: String, _config_version: String) -> Result<()> {
    println!("开始检查应用更新: {}", name);

    // 1. 获取锁文件中记录的已安装版本
    let lock_version = get_package_version_from_lock(&name)
        .ok()
        .flatten()
        .and_then(|v| parse_version(&v));

    println!("  锁文件中的版本: {:?}", lock_version);

    // 2. 获取远程版本列表（从 S3 下载 packages.json）
    let packages_s3_path = "app/packages.json";
    println!("  从 S3 获取版本列表: {}", packages_s3_path);

    let temp_dir = std::env::temp_dir().join("piksel_app_temp");
    std::fs::create_dir_all(&temp_dir)
        .context("创建临时目录失败")?;
    let temp_file = temp_dir.join("packages.json");

    if !download_file(packages_s3_path, temp_dir.to_str().unwrap()).await {
        anyhow::bail!("下载 packages.json 失败");
    }

    let packages_content = std::fs::read_to_string(&temp_file)
        .context("读取 packages.json 失败")?;
    let remote_info: RemoteInfo = serde_json::from_str(&packages_content)
        .context("解析 packages.json 失败")?;

    // 清理临时文件
    let _ = std::fs::remove_file(&temp_file);
    let _ = std::fs::remove_dir(&temp_dir);

    let remote_version = remote_info.packages.get(&name)
        .ok_or_else(|| anyhow!("远程版本列表中没有找到包: {}", name))?;
    let remote_version_parsed = parse_version(remote_version)
        .unwrap_or_else(|| remote_version.clone());
    println!("  远程应用版本: {}", remote_version_parsed);

    // 3. 判断是否需要更新
    // 如果锁文件版本不存在，或锁文件版本低于远程版本，则更新
    let need_update = match lock_version {
        None => {
            println!("  锁文件中无该包记录，视为首次安装，需要更新");
            true
        }
        Some(local) => {
            if local == remote_version_parsed {
                println!("  已是最新版本 (本地 {} == 远程 {})", local, remote_version_parsed);
                false
            } else if check_version(&local, &remote_version_parsed) {
                println!("  需要更新: 本地 {} -> 远程 {}", local, remote_version_parsed);
                true
            } else {
                println!("  本地版本 {} 高于远程 {}，无需更新", local, remote_version_parsed);
                false
            }
        }
    };

    if !need_update {
        return Ok(());
    }

    // 4. 获取远程包详细信息（从 S3 下载 dytes.json）
    let dytes_s3_path = format!("app/{}/dytes.json", name);
    println!("  从 S3 获取包详情: {}", dytes_s3_path);

    let temp_dir = std::env::temp_dir().join("piksel_app_detail_temp");
    std::fs::create_dir_all(&temp_dir)
        .context("创建临时目录失败")?;
    let temp_file = temp_dir.join("dytes.json");

    if !download_file(&dytes_s3_path, temp_dir.to_str().unwrap()).await {
        anyhow::bail!("下载 {}/dytes.json 失败", name);
    }

    let remote_app_info_text = std::fs::read_to_string(&temp_file)
        .context("读取远程 dytes.json 失败")?;
    let remote_app_info: DytesConfig = serde_json::from_str(&remote_app_info_text)
        .context("解析远程 dytes.json 失败")?;

    let _ = std::fs::remove_file(&temp_file);
    let _ = std::fs::remove_dir(&temp_dir);

    println!("  远程版本: {}", remote_app_info.version);
    println!("  导出文件: {:?}", remote_app_info.exports);

    // 5. 获取应用目录
    let app_dir = get_app_dir();
    std::fs::create_dir_all(&app_dir)
        .context("创建应用目录失败")?;
    println!("  应用目录: {}", app_dir);

    // 6. 下载导出的文件（失败即中止）
    for (_source_path, dest_filename) in &remote_app_info.exports {
        let is_html = is_html_file(dest_filename);
        let target_dir = if is_html {
            let web_dir = format!("{}/web", app_dir);
            std::fs::create_dir_all(&web_dir)
                .context("创建 web 目录失败")?;
            web_dir
        } else {
            app_dir.clone()
        };

        let local_filename = get_filename_from_path(dest_filename);
        let remote_filename = local_filename.clone();

        let s3_path = format!("app/{}/{}/{}", name, remote_app_info.version, remote_filename);
        let dest_path = format!("{}/{}", target_dir, local_filename);

        println!("  下载: {} -> {}", s3_path, dest_path);

        if !download_file(&s3_path, &target_dir).await {
            anyhow::bail!("下载 {} 失败", remote_filename);
        }

        let downloaded_path = format!("{}/{}", target_dir, remote_filename);
        if downloaded_path != dest_path && std::path::Path::new(&downloaded_path).exists() {
            if std::path::Path::new(&dest_path).exists() {
                std::fs::remove_file(&dest_path)
                    .context("删除旧目标文件失败")?;
            }
            std::fs::rename(&downloaded_path, &dest_path)
                .context("重命名下载文件失败")?;
            println!("  重命名: {} -> {}", remote_filename, local_filename);
        }

        println!("  下载完成: {}", local_filename);
    }

    // 7. 构建 loader 数据
    let mut loader = Vec::new();
    for (source, dest) in &remote_app_info.exports {
        loader.push(vec![source.clone(), dest.clone()]);
    }

    // 8. 写入锁文件
    update_package_in_lock(
        &name,
        &remote_app_info.version,
        "app",
        loader,
    ).context("更新锁文件失败")?;

    println!("  应用更新完成: {} v{}", name, remote_app_info.version);
    Ok(())
}