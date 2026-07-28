use anyhow::{Result, Context};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::utils::loader::download_file;
use crate::utils::env::get_app_dir;
use crate::utils::lock::update_package_in_lock;
use crate::utils::dytes::DytesConfig;

/// 生成随机数（基于时间戳）
fn get_random_param() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("_t={}", timestamp)
}

/// Web 资源更新结果，包含是否需要执行 init 脚本
pub struct WebUpdateResult {
    pub package_name: String,
    pub web_dir: String,
    pub init_scripts: Vec<String>,
}

pub async fn update_webs_info(name: String, version: String) -> Result<Option<WebUpdateResult>> {
    println!("更新 Web 资源信息: {} v{}", name, version);
    
    // 1. 获取远程包信息（使用 S3 路径）
    let s3_path = format!("webs/{}/dytes.json", name);
    println!("  从 S3 获取: {}", s3_path);
    
    // 创建临时目录存放下载的 dytes.json
    let temp_dir = std::env::temp_dir().join("piksel_web_temp");
    fs::create_dir_all(&temp_dir)?;
    let temp_file = temp_dir.join("dytes.json");
    
    // 使用 download_file 下载（自动识别 S3 路径）
    if !download_file(&s3_path, temp_dir.to_str().unwrap()).await {
        eprintln!("获取 {}/dytes.json 失败，跳过", name);
        return Ok(None);
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
    
    // 2. 获取应用目录 - web/{库名}/
    let app_dir = get_app_dir();
    let web_dir = format!("{}/web/{}", app_dir, name);
    fs::create_dir_all(&web_dir)?;
    
    // 3. 下载导出的文件（使用 S3 路径）
    for (_source_path, dest_filename) in &remote_app_info.exports {
        let s3_path = format!("webs/{}/{}/{}", name, remote_app_info.version, dest_filename);
        let dest_path = format!("{}/{}", web_dir, dest_filename);
        
        println!("  下载: {} -> {}", s3_path, dest_path);
        
        if !download_file(&s3_path, &web_dir).await {
            eprintln!("  下载 {} 失败", dest_filename);
            continue;
        }
        
        println!("  下载完成: {}", dest_filename);
    }
    
    // 4. 如果有 init 脚本，也下载（使用 S3 路径）
    for init_script in &remote_app_info.init {
        let filename = Path::new(init_script)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        
        let s3_path = format!("webs/{}/{}/{}", name, remote_app_info.version, filename);
        let dest_path = format!("{}/{}", web_dir, filename);
        
        println!("  下载初始化脚本: {} -> {}", s3_path, dest_path);
        
        if !download_file(&s3_path, &web_dir).await {
            eprintln!("  下载初始化脚本 {} 失败", init_script);
        }
    }
    
    // 5. 构建 loader 数据
    let mut loader = Vec::new();
    for (source, dest) in &remote_app_info.exports {
        loader.push(vec![source.clone(), dest.clone()]);
    }
    
    // 6. 写入锁文件
    update_package_in_lock(
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