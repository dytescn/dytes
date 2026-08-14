use anyhow::{Result, Context};
use std::fs;
use crate::utils::loader::download_file;
use crate::utils::env::get_app_dir;
use crate::utils::lock::update_package_in_lock;
use crate::utils::dytes::DytesConfig;

pub async fn update_os_info(name: String, version: String) -> Result<()> {
    println!("更新操作系统信息: {} v{}", name, version);
    
    // 1. 获取远程包信息（使用 S3 路径）
    let s3_path = format!("core/{}/dytes.json", name);
    println!("  从 S3 获取: {}", s3_path);
    
    // 创建临时目录存放下载的 dytes.json
    let temp_dir = std::env::temp_dir().join("piksel_core_temp");
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
    
    // 2. 获取应用目录 - 直接放到 ./
    let app_dir = get_app_dir();
    fs::create_dir_all(&app_dir)?;
    
    // 3. 下载导出的文件（使用 S3 路径）
    for (_source_path, dest_filename) in &remote_app_info.exports {
        let s3_path = format!("core/{}/{}/{}", name, remote_app_info.version, dest_filename);
        
        // 下载到 ./{文件名}，确保路径正确
        let dest_path = format!("{}/{}", app_dir.trim_end_matches('/'), dest_filename);
        
        println!("  下载: {} -> {}", s3_path, dest_path);
        
        // 下载文件到 app_dir
        if !download_file(&s3_path, &app_dir).await {
            eprintln!("  下载 {} 失败", dest_filename);
            continue;
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
        "core",
        loader,
    )?;
    
    println!("  更新完成: {} v{}", name, remote_app_info.version);
    Ok(())
}