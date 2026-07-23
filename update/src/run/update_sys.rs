use anyhow::{Result, Context};
use std::fs;
use crate::utils::loader::download_file;
use crate::utils::env::get_app_dir;
use crate::utils::lock::update_package_in_lock;
use crate::utils::dytes::DytesConfig;

pub async fn update_sys_info(name: String, version: String) -> Result<()> {
    println!("更新系统库信息: {} v{}", name, version);
    
    // 1. 获取远程包信息
    let detail_url = format!("https://packages.funxdata.com/system/{}/dytes.json", name);
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
    
    // 2. 获取应用目录 - library/{库名}/
    let app_dir = get_app_dir();
    let lib_dir = format!("{}/library/{}", app_dir, name);
    fs::create_dir_all(&lib_dir)?;
    
    // 3. 下载导出的文件
    for (_source_path, dest_filename) in &remote_app_info.exports {
        // 直接使用 dest_filename 作为下载文件名
        let remote_url = format!(
            "https://packages.funxdata.com/system/{}/{}/{}",
            name, remote_app_info.version, dest_filename
        );
        
        // 下载到 library/{库名}/{文件名}
        let dest_path = format!("{}/{}", lib_dir, dest_filename);
        
        println!("  下载: {} -> {}", remote_url, dest_path);
        
        // 下载文件到 lib_dir
        if !download_file(&remote_url, &lib_dir).await {
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
        "sys",
        loader,
    )?;
    
    println!("  系统库更新完成: {} v{}", name, remote_app_info.version);
    Ok(())
}