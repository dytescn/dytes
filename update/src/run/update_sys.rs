use anyhow::{Result, Context};
use std::fs;
use crate::utils::env::get_app_dir;
use crate::utils::lock::update_package_in_lock;
use crate::utils::dytes::DytesConfig;
use crate::utils::loader::{download_file, get_random_param};

/// 获取当前操作系统对应的库文件扩展名
fn get_lib_extension() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "dll"
    }
    #[cfg(target_os = "macos")]
    {
        "dylib"
    }
    #[cfg(target_os = "linux")]
    {
        "so"
    }
}

/// 根据操作系统匹配库文件名
fn match_lib_filename(dest_filename: &str) -> String {
    let ext = get_lib_extension();
    
    // 如果文件名已经包含扩展名，替换为当前系统的扩展名
    if dest_filename.contains('.') {
        if let Some(base_name) = dest_filename.split('.').next() {
            return format!("{}.{}", base_name, ext);
        }
    }
    
    // 如果没有扩展名，直接添加
    format!("{}.{}", dest_filename, ext)
}

pub async fn update_sys_info(name: String, version: String) -> Result<()> {
    println!("更新系统库信息: {} v{}", name, version);
    
    let rand = get_random_param();
    // 1. 获取远程包信息
    let detail_url = format!("https://packages.funxdata.com/system/{}/dytes.json?rand={}", name, rand);
    let detail_response = reqwest::get(&detail_url).await
        .with_context(|| format!("请求 {} 失败", detail_url))?;
    
    if !detail_response.status().is_success() {
        eprintln!("获取 {}/dytes.json 失败，跳过", name);
        return Ok(());
    }
    
    let remote_app_info_text = detail_response.text().await?;
    let remote_app_info: DytesConfig = serde_json::from_str(&remote_app_info_text)
        .with_context(|| format!("解析 {}/dytes.json 失败", name))?;
    
    println!("  远程版本: {}", remote_app_info.version);
    println!("  导出文件: {:?}", remote_app_info.exports);
    
    // 2. 获取应用目录 - library/
    let app_dir = get_app_dir();
    let lib_dir = format!("{}/libary", app_dir.trim_end_matches('/'));
    fs::create_dir_all(&lib_dir)?;
    
    // 3. 下载导出的文件
    for (_source_path, dest_filename) in &remote_app_info.exports {
        // 根据操作系统匹配库文件名
        let remote_filename = match_lib_filename(dest_filename);
        
        // 构建远程 URL
        let rand = get_random_param();
        let remote_url = format!(
            "https://packages.funxdata.com/system/{}/{}/{}?rand={}",
            name, remote_app_info.version, remote_filename, rand
        );
        
        // 本地目标路径 - 使用原始 dest_filename（保持配置文件中的名称）
        let dest_path = format!("{}/{}", lib_dir, dest_filename);
        
        println!("  下载: {} -> {}", remote_url, dest_path);
        
        // 下载文件到 lib_dir
        if !download_file(&remote_url, &lib_dir).await {
            eprintln!("  下载 {} 失败", remote_filename);
            continue;
        }
        
        // 如果下载的文件名与目标文件名不同，需要重命名
        let downloaded_path = format!("{}/{}", lib_dir, remote_filename);
        if downloaded_path != dest_path && std::path::Path::new(&downloaded_path).exists() {
            if std::path::Path::new(&dest_path).exists() {
                std::fs::remove_file(&dest_path)?;
            }
            std::fs::rename(&downloaded_path, &dest_path)?;
            println!("  重命名: {} -> {}", remote_filename, dest_filename);
        }
        
        println!("  下载完成: {}", dest_filename);
    }
    
    // 4. 构建 loader 数据
    let mut loader = Vec::new();
    for (source, dest) in &remote_app_info.exports {
        loader.push(vec![source.clone(), dest.clone()]);
    }
    
    // 5. 写入锁文件 - 使用 utils::lock
    update_package_in_lock(
        &name,
        &remote_app_info.version,
        "sys",
        loader,
    )?;
    
    println!("  系统库更新完成: {} v{}", name, remote_app_info.version);
    Ok(())
}