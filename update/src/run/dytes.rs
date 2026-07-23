use super::{update_app, update_sys, update_os, update_webs};
use anyhow::Result;
use crate::utils::lock::{read_lock, get_package_version_from_lock};
use crate::utils::version::check_version;
use crate::utils::dytes::{DytesConfig, read_dytes_config, print_config_info};
use super::update_setup::setup_webs_init;

/// 进度回调类型
pub type ProgressCallback = Box<dyn Fn(f32, &str) + Send + Sync>;

/// 解析版本号，去除 ^ 或 ~ 前缀
fn parse_version(version: &str) -> String {
    version.trim_start_matches('^').trim_start_matches('~').to_string()
}

/// 获取应用名称
pub fn get_app_name() -> Result<String> {
    let config = read_dytes_config()?;
    Ok(config.name)
}

/// 检查依赖是否需要更新
fn need_update(name: &str, remote_version: &str) -> bool {
    let lock_version = match get_package_version_from_lock(name) {
        Ok(Some(v)) => v,
        _ => {
            println!("  {} 不在锁文件中，需要更新", name);
            return true;
        }
    };
    
    let lock_parsed = parse_version(&lock_version);
    let remote_parsed = parse_version(remote_version);
    
    if lock_parsed == remote_parsed {
        println!("  {} 已是最新版本 (v{})", name, lock_parsed);
        return false;
    }
    
    if check_version(&lock_parsed, &remote_parsed) {
        println!("  {} 需要更新: 锁文件 v{} -> 远程 v{}", name, lock_parsed, remote_parsed);
        return true;
    }
    
    println!("  {} 锁文件版本 v{} 高于远程 v{}，无需更新", name, lock_parsed, remote_parsed);
    false
}

/// 检查是否有更新
pub async fn check_updates() -> Result<bool> {
    let config = read_dytes_config()?;
    
    let core_deps = config.dependencies_core.iter().collect::<Vec<_>>();
    let sys_deps = config.dependencies_sys.iter().collect::<Vec<_>>();
    let web_deps = config.dependencies_web.iter().collect::<Vec<_>>();
    
    for (name, version) in core_deps {
        if need_update(name, version) {
            return Ok(true);
        }
    }
    for (name, version) in sys_deps {
        if need_update(name, version) {
            return Ok(true);
        }
    }
    for (name, version) in web_deps {
        if need_update(name, version) {
            return Ok(true);
        }
    }
    
    Ok(false)
}

/// 更新依赖列表
fn collect_updates<'a>(
    deps: &'a [(&'a String, &'a String)],
    dep_type: &str,
) -> Vec<(String, String, String)> {
    let mut updates = Vec::new();
    for (name, version) in deps {
        if need_update(name, version) {
            updates.push((dep_type.to_string(), (*name).clone(), (*version).clone()));
        }
    }
    updates
}

pub async fn init_dytes() -> Result<()> {
    init_dytes_with_progress(None).await
}

pub async fn init_dytes_with_progress(progress_callback: Option<ProgressCallback>) -> Result<()> {
    println!("初始化更新程序...");
    
    update_progress(&progress_callback, 0.0, "读取配置文件...");
    
    let config = read_dytes_config()?;
    print_config_info(&config);
    
    let lock = read_lock().unwrap_or_default();
    println!("\n锁文件状态: {} 个包已安装", lock.package.len());
    
    let core_deps: Vec<_> = config.dependencies_core.iter().collect();
    let sys_deps: Vec<_> = config.dependencies_sys.iter().collect();
    let web_deps: Vec<_> = config.dependencies_web.iter().collect();
    
    let mut need_update_list = Vec::new();
    need_update_list.extend(collect_updates(&core_deps, "core"));
    need_update_list.extend(collect_updates(&sys_deps, "sys"));
    need_update_list.extend(collect_updates(&web_deps, "web"));
    
    if need_update_list.is_empty() {
        println!("\n所有依赖已是最新版本，无需更新");
        update_progress(&progress_callback, 100.0, "已是最新版本");
        return Ok(());
    }
    
    println!("\n发现 {} 个依赖需要更新", need_update_list.len());
    
    let total_deps = core_deps.len() + sys_deps.len() + web_deps.len();
    let mut completed = 0;
    
    // 1. 更新应用本身
    update_progress(&progress_callback, 5.0, "检查应用本身更新...");
    if let Err(e) = update_app::update_app_info(config.name.clone(), config.version.clone()).await {
        println!("  应用本身更新失败: {}", e);
    }
    completed += 1;
    
    // 2. 核心依赖更新
    if !core_deps.is_empty() {
        update_progress(&progress_callback, 10.0, "检查核心依赖更新...");
        for (name, version) in core_deps {
            if need_update(name, version) {
                if let Err(e) = update_os::update_os_info(name.clone(), version.clone()).await {
                    println!("  更新 {} 失败: {}", name, e);
                }
            } else {
                println!("  跳过 {} (已是最新)", name);
            }
            completed += 1;
            let progress = 10.0 + (completed as f32 / total_deps as f32) * 60.0;
            update_progress(&progress_callback, progress, &format!("更新核心: {}", name));
        }
    }
    
    // 3. 系统库更新
    if !sys_deps.is_empty() {
        update_progress(&progress_callback, 70.0, "检查系统库更新...");
        for (name, version) in sys_deps {
            if need_update(name, version) {
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
    
    // 4. Web 资源更新 - 收集需要执行 init 的包
    let mut web_init_results = Vec::new();
    if !web_deps.is_empty() {
        update_progress(&progress_callback, 85.0, "检查 Web 资源更新...");
        for (name, version) in web_deps {
            if need_update(name, version) {
                match update_webs::update_webs_info(name.clone(), version.clone()).await {
                    Ok(Some(result)) => {
                        web_init_results.push(result);
                    }
                    Ok(None) => {
                        println!("  {} 无需执行 init 脚本", name);
                    }
                    Err(e) => {
                        println!("  更新 {} 失败: {}", name, e);
                    }
                }
            } else {
                println!("  跳过 {} (已是最新)", name);
            }
            completed += 1;
            let progress = 85.0 + (completed as f32 / total_deps as f32) * 10.0;
            update_progress(&progress_callback, progress, &format!("更新 Web 资源: {}", name));
        }
    }
    
    // 5. 所有 Web 资源更新完成后，统一执行 init 脚本
    if !web_init_results.is_empty() {
        println!("\n执行 Web 资源初始化脚本...");
        for result in web_init_results {
            if let Err(e) = setup_webs_init(&result.web_dir, &result.init_scripts) {
                eprintln!("  执行 {} 的 init 脚本失败: {}", result.package_name, e);
            }
        }
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
    read_dytes_config()
}