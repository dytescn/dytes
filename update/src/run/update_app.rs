use std::collections::HashMap;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::utils::version::check_version;
use crate::utils::loader::download_file;
use crate::utils::env::get_app_dir;
use crate::utils::lock::update_package_in_lock;
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
        return Some(cleaned.to_string());
    }
    None
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

/// 获取本地文件名（{app_name}.exe）
fn get_local_filename(app_name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{}.exe", app_name)
    }
    #[cfg(target_os = "macos")]
    {
        app_name.to_string()
    }
    #[cfg(target_os = "linux")]
    {
        app_name.to_string()
    }
}

/// 获取远程文件名（根据操作系统匹配）
fn get_remote_filename(app_name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{}{}", app_name, "_windows.exe")
    }
    #[cfg(target_os = "macos")]
    {
        format!("{}{}", app_name, "_macos")
    }
    #[cfg(target_os = "linux")]
    {
        format!("{}{}", app_name, "_linux")
    }
}

pub async fn update_app_info(name: String, version: String) -> Result<()> {
    println!("开始检查应用更新: {} v{}", name, version);
    
    // 1. 获取远程版本列表（从 S3 下载 packages.json）
    let packages_s3_path = "app/packages.json";
    println!("  从 S3 获取版本列表: {}", packages_s3_path);
    
    // 创建临时目录存放下载的 packages.json
    let temp_dir = std::env::temp_dir().join("piksel_app_temp");
    std::fs::create_dir_all(&temp_dir)?;
    let temp_file = temp_dir.join("packages.json");
    
    // 下载 packages.json
    if !download_file(packages_s3_path, temp_dir.to_str().unwrap()).await {
        anyhow::bail!("下载 packages.json 失败");
    }
    
    // 读取 packages.json
    let packages_content = std::fs::read_to_string(&temp_file)?;
    let remote_info: RemoteInfo = serde_json::from_str(&packages_content)?;
    
    // 清理临时文件
    let _ = std::fs::remove_file(&temp_file);
    let _ = std::fs::remove_dir(&temp_dir);
    
    let remote_version = remote_info.packages.get(&name).unwrap_or(&version);
    println!("  远程应用版本: {:?}", remote_version);
    
    // 2. 解析版本进行比较
    let local_version = parse_version(&version).unwrap_or_else(|| version.clone());
    let remote_version_str = remote_version.as_str();
    
    if !check_version(&local_version, remote_version_str) {
        println!("  当前已是最新版本");
        return Ok(());
    }
    
    // 3. 获取远程包详细信息（从 S3 下载 dytes.json）
    println!("  发现新版本 {} -> {}，开始准备更新...", local_version, remote_version_str);
    let dytes_s3_path = format!("app/{}/dytes.json", name);
    println!("  从 S3 获取包详情: {}", dytes_s3_path);
    
    let temp_dir = std::env::temp_dir().join("piksel_app_detail_temp");
    std::fs::create_dir_all(&temp_dir)?;
    let temp_file = temp_dir.join("dytes.json");
    
    if !download_file(&dytes_s3_path, temp_dir.to_str().unwrap()).await {
        eprintln!("获取 {}/dytes.json 失败，跳过", name);
        return Ok(());
    }
    
    let remote_app_info_text = std::fs::read_to_string(&temp_file)?;
    let remote_app_info: DytesConfig = serde_json::from_str(&remote_app_info_text)?;
    
    // 清理临时文件
    let _ = std::fs::remove_file(&temp_file);
    let _ = std::fs::remove_dir(&temp_dir);
    
    println!("  远程版本: {}", remote_app_info.version);
    println!("  导出文件: {:?}", remote_app_info.exports);
    
    // 4. 获取应用目录
    let app_dir = get_app_dir();
    std::fs::create_dir_all(&app_dir)?;
    
    // 5. 下载导出的文件（使用 S3 路径）
    for (_source_path, dest_filename) in &remote_app_info.exports {
        // 判断是否为 HTML 文件
        let is_html = is_html_file(dest_filename);
        
        // 确定目标目录
        let target_dir = if is_html {
            let web_dir = format!("{}/web", app_dir);
            std::fs::create_dir_all(&web_dir)?;
            web_dir
        } else {
            app_dir.clone()
        };
        
        // 获取本地文件名
        let local_filename = get_filename_from_path(dest_filename);
        
        // 如果是 piksel 相关的可执行文件，使用 app_name 构建文件名
        let local_filename = if local_filename.contains("piksel") && local_filename.ends_with(".exe") {
            get_local_filename(&name)
        } else {
            local_filename
        };
        
        // 获取远程文件名（根据操作系统匹配）
        let remote_filename = if local_filename.contains(&name) && local_filename.ends_with(".exe") {
            get_remote_filename(&name)
        } else {
            local_filename.clone()
        };
        
        // 构建 S3 路径
        let s3_path = format!("app/{}/{}/{}", name, remote_app_info.version, remote_filename);
        
        // 本地目标路径
        let dest_path = format!("{}/{}", target_dir, local_filename);
        
        println!("  下载: {} -> {}", s3_path, dest_path);
        
        // 下载文件到目标目录
        if !download_file(&s3_path, &target_dir).await {
            eprintln!("  下载 {} 失败", remote_filename);
            continue;
        }
        
        // 如果下载的文件名与目标文件名不同，需要重命名
        let downloaded_path = format!("{}/{}", target_dir, remote_filename);
        
        if downloaded_path != dest_path && std::path::Path::new(&downloaded_path).exists() {
            // 如果目标文件已存在，先删除
            if std::path::Path::new(&dest_path).exists() {
                std::fs::remove_file(&dest_path)?;
            }
            std::fs::rename(&downloaded_path, &dest_path)?;
            println!("  重命名: {} -> {}", remote_filename, local_filename);
        }
        
        println!("  下载完成: {}", local_filename);
    }
    
    // 6. 构建 loader 数据
    let mut loader = Vec::new();
    for (source, dest) in &remote_app_info.exports {
        loader.push(vec![source.clone(), dest.clone()]);
    }
    
    // 7. 写入锁文件 - 使用 utils::lock
    update_package_in_lock(
        &name,
        &remote_app_info.version,
        "app",
        loader,
    )?;
    
    println!("  应用更新完成: {} v{}", name, remote_app_info.version);
    Ok(())
}