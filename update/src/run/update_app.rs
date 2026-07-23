use std::collections::HashMap;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::utils::version::check_version;
use crate::utils::loader::{download_file, get_random_param};
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

/// 获取当前操作系统对应的可执行文件后缀
fn get_os_executable_suffix() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "_windows.exe"
    }
    #[cfg(target_os = "macos")]
    {
        "_macos"
    }
    #[cfg(target_os = "linux")]
    {
        "_linux"
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
    
    let rand = get_random_param();
    
    // 1. 获取远程版本列表
    let url = format!("https://packages.funxdata.com/app/packages.json?rand={}", rand);
    let response = reqwest::get(&url).await?;
    if !response.status().is_success() {
        anyhow::bail!("下载 packages.json 失败: HTTP {}", response.status());
    }
    let dytes_app_remote_json_text = response.text().await?;
    let dytes_app_remote_json: RemoteInfo = serde_json::from_str(&dytes_app_remote_json_text)?;
    let remote_version = dytes_app_remote_json.packages.get(&name).unwrap_or(&version);
    println!("  远程应用版本: {:?}", remote_version);
    
    // 2. 解析版本进行比较
    let local_version = parse_version(&version).unwrap_or_else(|| version.clone());
    let remote_version_str = remote_version.as_str();
    
    if !check_version(&local_version, remote_version_str) {
        println!("  当前已是最新版本");
        return Ok(());
    }
    
    // 3. 获取远程包详细信息
    println!("  发现新版本 {} -> {}，开始准备更新...", local_version, remote_version_str);
    let rand = get_random_param();
    let detail_url = format!("https://packages.funxdata.com/app/{}/dytes.json?rand={}", name, rand);
    let detail_response = reqwest::get(&detail_url).await?;
    if !detail_response.status().is_success() {
        eprintln!("获取 {}/dytes.json 失败，跳过", name);
        return Ok(());
    }
    let remote_app_info_text = detail_response.text().await?;
    let remote_app_info: DytesConfig = serde_json::from_str(&remote_app_info_text)?;
    
    println!("  远程版本: {}", remote_app_info.version);
    println!("  导出文件: {:?}", remote_app_info.exports);
    
    // 4. 获取应用目录
    let app_dir = get_app_dir();
    std::fs::create_dir_all(&app_dir)?;
    
    // 5. 下载导出的文件
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
        
        // 构建远程 URL（添加随机参数）
        let rand = get_random_param();
        let remote_url = format!(
            "https://packages.funxdata.com/app/{}/{}/{}?rand={}",
            name, remote_app_info.version, remote_filename, rand
        );
        
        // 本地目标路径
        let dest_path = format!("{}/{}", target_dir, local_filename);
        
        println!("  下载: {} -> {}", remote_url, dest_path);
        
        // 下载文件到目标目录
        if !download_file(&remote_url, &target_dir).await {
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