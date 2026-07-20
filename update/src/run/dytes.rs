use super::{update_app, update_sys, update_os, update_webs};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// 进度回调类型
pub type ProgressCallback = Box<dyn Fn(f32, &str) + Send + Sync>;

#[derive(Debug, Deserialize, Serialize)]
pub struct DytesConfig {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    #[serde(rename = "type")]
    pub app_type: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    
    #[serde(default)]
    #[serde(rename = "dependencies-core")]
    pub dependencies_core: HashMap<String, String>,
    
    #[serde(default)]
    #[serde(rename = "dependencies-sys")]
    pub dependencies_sys: HashMap<String, String>,
    
    #[serde(default)]
    #[serde(rename = "dependencies-web")]
    pub dependencies_web: HashMap<String, String>,
}

pub async fn init_dytes() -> Result<()> {
    init_dytes_with_progress(None).await
}

pub async fn init_dytes_with_progress(progress_callback: Option<ProgressCallback>) -> Result<()> {
    println!("初始化更新程序...");
    
    update_progress(&progress_callback, 0.0, "读取配置文件...");
    
    let config = load_config()?;
    print_config_info(&config);
    
    let core_deps: Vec<_> = config.dependencies_core.iter().collect();
    let sys_deps: Vec<_> = config.dependencies_sys.iter().collect();
    let web_deps: Vec<_> = config.dependencies_web.iter().collect();
    
    let total_deps = core_deps.len() + sys_deps.len() + web_deps.len();
    let mut completed = 0;
    
    // 1. 检查应用更新 (dependencies-core)
    if !core_deps.is_empty() {
        update_progress(&progress_callback, 10.0, "检查应用更新...");
        for (name, version) in core_deps {
            if let Err(e) = update_app::update_app_info(name.clone(), version.clone()).await {
                println!("  更新 {} 失败: {}", name, e);
            }
            completed += 1;
            let progress = 10.0 + (completed as f32 / total_deps as f32) * 60.0;
            update_progress(&progress_callback, progress, &format!("更新应用: {}", name));
        }
    }
    
    // 2. 检查系统库更新 (dependencies-sys)
    if !sys_deps.is_empty() {
        update_progress(&progress_callback, 70.0, "检查系统库更新...");
        for (name, version) in sys_deps {
            if let Err(e) = update_sys::update_sys_info(name.clone(), version.clone()).await {
                println!("  更新 {} 失败: {}", name, e);
            }
            completed += 1;
            let progress = 70.0 + (completed as f32 / total_deps as f32) * 15.0;
            update_progress(&progress_callback, progress, &format!("更新系统库: {}", name));
        }
    }
    
    // 3. 检查 Web 资源更新 (dependencies-web)
    if !web_deps.is_empty() {
        update_progress(&progress_callback, 85.0, "检查 Web 资源更新...");
        for (name, version) in web_deps {
            if let Err(e) = update_webs::update_webs_info(name.clone(), version.clone()).await {
                println!("  更新 {} 失败: {}", name, e);
            }
            completed += 1;
            let progress = 85.0 + (completed as f32 / total_deps as f32) * 10.0;
            update_progress(&progress_callback, progress, &format!("更新 Web 资源: {}", name));
        }
    }
    
    // 4. 检查操作系统更新
    update_progress(&progress_callback, 95.0, "检查操作系统更新...");
    if let Err(e) = update_os::update_os_info("os".to_string(), "latest".to_string()).await {
        println!("  操作系统更新失败: {}", e);
    }
    
    update_progress(&progress_callback, 100.0, "更新完成");
    println!("\n所有更新检查完成");
    Ok(())
}

fn update_progress(callback: &Option<ProgressCallback>, progress: f32, status: &str) {
    if let Some(cb) = callback {
        cb(progress, status);
    }
    println!("进度: {:.1}% - {}", progress, status);
}

pub fn load_config() -> Result<DytesConfig> {
    let config_path = "dytes.json";
    if !Path::new(config_path).exists() {
        anyhow::bail!("配置文件 dytes.json 不存在");
    }
    
    let content = fs::read_to_string(config_path)
        .with_context(|| format!("读取配置文件 {} 失败", config_path))?;
    
    let config: DytesConfig = serde_json::from_str(&content)
        .with_context(|| "解析配置文件失败")?;
    
    Ok(config)
}

fn print_config_info(config: &DytesConfig) {
    println!("应用信息:");
    println!("  名称: {}", config.name);
    println!("  版本: {}", config.version);
    if let Some(desc) = &config.description {
        println!("  描述: {}", desc);
    }
    if let Some(author) = &config.author {
        println!("  作者: {}", author);
    }
    if let Some(license) = &config.license {
        println!("  许可证: {}", license);
    }
    
    println!("\n依赖统计:");
    println!("  核心依赖 (应用): {} 个", config.dependencies_core.len());
    println!("  系统依赖 (库): {} 个", config.dependencies_sys.len());
    println!("  Web 依赖: {} 个", config.dependencies_web.len());
}