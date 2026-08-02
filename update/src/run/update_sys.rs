use anyhow::{Result, Context};
use std::fs;
use std::path::Path;
use crate::utils::env::get_app_dir;
use crate::utils::lock::update_package_in_lock;
use crate::utils::dytes::DytesConfig;
use crate::utils::loader::download_file;

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
    
    // 1. 获取远程包信息（使用 S3 路径）
    let s3_path = format!("system/{}/dytes.json", name);
    println!("  从 S3 获取: {}", s3_path);
    
    // 创建临时目录存放下载的 dytes.json
    let temp_dir = std::env::temp_dir().join("piksel_sys_temp");
    fs::create_dir_all(&temp_dir)?;
    let temp_file = temp_dir.join("dytes.json");
    
    // 使用 download_file 下载（自动识别 S3 路径）
    if !download_file(&s3_path, temp_dir.to_str().unwrap()).await {
        eprintln!("获取 {}/dytes.json 失败，跳过", name);
        return Ok(());
    }
    
    // 读取下载的 dytes.json
    let remote_app_info_text = fs::read_to_string(&temp_file)?;
    let remote_app_info: DytesConfig = serde_json::from_str(&remote_app_info_text)
        .with_context(|| format!("解析 {}/dytes.json 失败", name))?;
    
    // 清理临时文件
    let _ = fs::remove_file(&temp_file);
    let _ = fs::remove_dir(&temp_dir);
    
    println!("  远程版本: {}", remote_app_info.version);
    println!("  导出文件: {:?}", remote_app_info.exports);
    
    // 2. 获取应用目录 - libary/
    let app_dir = get_app_dir();
    let lib_dir = format!("{}/libary", app_dir.trim_end_matches('/'));
    fs::create_dir_all(&lib_dir)?;
    
    // 3. 下载导出的文件（使用 S3 路径）
    for (_source_path, dest_filename) in &remote_app_info.exports {
        // 根据操作系统匹配库文件名
        let remote_filename = match_lib_filename(dest_filename);
        
        // 构建 S3 路径
        let s3_path = format!("system/{}/{}/{}", name, remote_app_info.version, remote_filename);
        
        // 本地目标路径 - 使用原始 dest_filename（保持配置文件中的名称）
        let dest_path = format!("{}/{}", lib_dir, dest_filename);
        
        println!("  下载: {} -> {}", s3_path, dest_path);
        
        // 下载文件到 lib_dir
        if !download_file(&s3_path, &lib_dir).await {
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