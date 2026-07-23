use super::{update_app, update_sys, update_os, update_webs};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use crate::utils::lock::{read_lock, get_package_version_from_lock};
use crate::utils::version::check_version;

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

/// 解析版本号，去除 ^ 或 ~ 前缀
fn parse_version(version: &str) -> String {
    version.trim_start_matches('^').trim_start_matches('~').to_string()
}

/// 检查依赖是否需要更新
fn need_update(name: &str, current_version: &str, remote_version: &str) -> bool {
    // 如果锁文件中没有该包，需要更新
    let lock_version = match get_package_version_from_lock(name) {
        Ok(Some(v)) => v,
        _ => {
            println!("  {} 不在锁文件中，需要更新", name);
            return true;
        }
    };
    
    // 解析版本进行比较
    let current_parsed = parse_version(current_version);
    let lock_parsed = parse_version(&lock_version);
    let remote_parsed = parse_version(remote_version);
    
    // 如果锁文件版本与远程版本相同，且与当前配置一致，则不需要更新
    if lock_parsed == remote_parsed && remote_parsed == current_parsed {
        println!("  {} 已是最新版本 (v{})", name, lock_parsed);
        return false;
    }
    
    println!("  {} 需要更新: 锁文件 v{} -> 远程 v{}", name, lock_parsed, remote_parsed);
    true
}

pub async fn init_dytes() -> Result<()> {
    init_dytes_with_progress(None).await
}

pub async fn init_dytes_with_progress(progress_callback: Option<ProgressCallback>) -> Result<()> {
    println!("初始化更新程序...");
    
    update_progress(&progress_callback, 0.0, "读取配置文件...");
    
    let config = load_config()?;
    print_config_info(&config);
    
    // 读取锁文件
    let lock = read_lock().unwrap_or_default();
    println!("\n锁文件状态: {} 个包已安装", lock.package.len());
    
    let core_deps: Vec<_> = config.dependencies_core.iter().collect();
    let sys_deps: Vec<_> = config.dependencies_sys.iter().collect();
    let web_deps: Vec<_> = config.dependencies_web.iter().collect();
    
    // 统计需要更新的包
    let mut need_update_list = Vec::new();
    
    // 检查核心依赖
    for (name, version) in &core_deps {
        // 获取远程版本（从 packages.json 获取，这里简化处理）
        // 实际应该从远程获取，这里用版本号比较
        let remote_version = version; // 暂时使用配置中的版本
        if need_update(name, version, remote_version) {
            need_update_list.push(("core".to_string(), name.clone(), version.clone()));
        }
    }
    
    // 检查系统库依赖
    for (name, version) in &sys_deps {
        let remote_version = version;
        if need_update(name, version, remote_version) {
            need_update_list.push(("sys".to_string(), name.clone(), version.clone()));
        }
    }
    
    // 检查 Web 依赖
    for (name, version) in &web_deps {
        let remote_version = version;
        if need_update(name, version, remote_version) {
            need_update_list.push(("web".to_string(), name.clone(), version.clone()));
        }
    }
    
    if need_update_list.is_empty() {
        println!("\n所有依赖已是最新版本，无需更新");
        update_progress(&progress_callback, 100.0, "已是最新版本");
        return Ok(());
    }
    
    println!("\n发现 {} 个依赖需要更新", need_update_list.len());
    
    let total_deps = core_deps.len() + sys_deps.len() + web_deps.len();
    let mut completed = 0;
    
    // 1. 检查应用更新 (dependencies-core)
    if !core_deps.is_empty() {
        update_progress(&progress_callback, 10.0, "检查应用更新...");
        for (name, version) in core_deps {
            // 检查是否需要更新
            if need_update(name, version, version) {
                if let Err(e) = update_app::update_app_info(name.clone(), version.clone()).await {
                    println!("  更新 {} 失败: {}", name, e);
                }
            } else {
                println!("  跳过 {} (已是最新)", name);
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
            if need_update(name, version, version) {
                if let Err(e) = update_sys::update_sys_info(name.clone(), version.clone()).await {
                    println!("  更新 {} 失败: {}", name, e);
                }
            } else {
                println!("  跳过 {} (已是最新)", name);
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
            if need_update(name, version, version) {
                if let Err(e) = update_webs::update_webs_info(name.clone(), version.clone()).await {
                    println!("  更新 {} 失败: {}", name, e);
                }
            } else {
                println!("  跳过 {} (已是最新)", name);
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