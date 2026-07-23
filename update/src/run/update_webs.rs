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
    
    let rand = get_random_param();
    
    // 1. 获取远程包信息
    let detail_url = format!("https://packages.funxdata.com/webs/{}/dytes.json?rand={}", name, rand);
    let detail_response = reqwest::get(&detail_url).await
        .with_context(|| format!("请求 {} 失败", detail_url))?;
    
    if !detail_response.status().is_success() {
        eprintln!("获取 {}/dytes.json 失败，跳过", name);
        return Ok(None);
    }
    
    let remote_app_info_text = detail_response.text().await?;
    let remote_app_info: DytesConfig = serde_json::from_str(&remote_app_info_text)
        .with_context(|| format!("解析 {}/dytes.json 失败", name))?;
    
    println!("  远程版本: {}", remote_app_info.version);
    println!("  导出文件: {:?}", remote_app_info.exports);
    
    // 2. 获取应用目录 - web/{库名}/
    let app_dir = get_app_dir();
    let web_dir = format!("{}/web/{}", app_dir, name);
    fs::create_dir_all(&web_dir)?;
    
    // 3. 下载导出的文件
    for (_source_path, dest_filename) in &remote_app_info.exports {
        let rand = get_random_param();
        let remote_url = format!(
            "https://packages.funxdata.com/webs/{}/{}/{}?rand={}",
            name, remote_app_info.version, dest_filename, rand
        );
        
        let dest_path = format!("{}/{}", web_dir, dest_filename);
        
        println!("  下载: {} -> {}", remote_url, dest_path);
        
        if !download_file(&remote_url, &web_dir).await {
            eprintln!("  下载 {} 失败", dest_filename);
            continue;
        }
        
        println!("  下载完成: {}", dest_filename);
    }
    
    // 4. 如果有 init 脚本，也下载
    for init_script in &remote_app_info.init {
        let filename = Path::new(init_script)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        
        let rand = get_random_param();
        let remote_url = format!(
            "https://packages.funxdata.com/webs/{}/{}/{}?rand={}",
            name, remote_app_info.version, filename, rand
        );
        
        let dest_path = format!("{}/{}", web_dir, filename);
        
        println!("  下载初始化脚本: {} -> {}", remote_url, dest_path);
        
        if !download_file(&remote_url, &web_dir).await {
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