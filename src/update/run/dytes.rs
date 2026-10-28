use super::update_setup::run_all_init_scripts;
use super::{update_app, update_design, update_model, update_os, update_sys, update_webs};
use crate::update::utils::dytes::{print_config_info, read_dytes_config, DytesConfig};
use crate::update::utils::env::get_app_dir;
use crate::update::utils::lock::{
    get_package_version_from_lock, read_lock, remove_package_from_lock,
};
use crate::update::utils::version::check_version;
use anyhow::{anyhow, Result};

/// 进度回调类型
pub type ProgressCallback = Box<dyn Fn(f32, &str) + Send + Sync>;

/// 解析版本号，去除 ^ 或 ~ 前缀
fn parse_version(version: &str) -> String {
    version
        .trim_start_matches('^')
        .trim_start_matches('~')
        .to_string()
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
        println!(
            "  {} 需要更新: 锁文件 v{} -> 远程 v{}",
            name, lock_parsed, remote_parsed
        );
        return true;
    }

    println!(
        "  {} 锁文件版本 v{} 高于远程 v{}，无需更新",
        name, lock_parsed, remote_parsed
    );
    false
}

/// 获取线上包的最新版本（读取 <type>/<name>/dytes.json）
async fn remote_version(pkg_type: &str, name: &str) -> Result<String> {
    let dir = match pkg_type {
        "core" => "core",
        "sys" => "system",
        "web" => "webs",
        "design_plugins" => "design_plugins",
        "design_soft" => "design_soft",
        "model" => "models",
        _ => "core",
    };

    // 优先使用 packages.json 索引中的版本
    let index_path = format!("{}/packages.json", dir);
    if let Ok(text) = crate::update::utils::loader::fetch_text(&index_path).await {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(version) = value
                .get("packages")
                .and_then(|v| v.get(name))
                .and_then(|v| v.as_str())
            {
                return Ok(version.to_string());
            }
        }
    }

    // 回退到包自身的 dytes.json
    let path = format!("{}/{}/dytes.json", dir, name);
    let text = crate::update::utils::loader::fetch_text(&path).await?;
    let config: DytesConfig = serde_json::from_str(&text)?;
    Ok(config.version)
}

/// 检查一组依赖在线上是否有更新
async fn deps_need_update(
    deps: &std::collections::HashMap<String, String>,
    pkg_type: &str,
) -> bool {
    for name in deps.keys() {
        match remote_version(pkg_type, name).await {
            Ok(remote) => {
                if need_update(name, &remote) {
                    return true;
                }
            }
            Err(e) => println!("  检查 {} 线上版本失败: {}", name, e),
        }
    }
    false
}

/// 删除锁文件中本地文件已缺失的包，强制重新下载
fn prune_missing_packages() {
    let Ok(lock) = read_lock() else {
        return;
    };
    let app_dir = get_app_dir();
    for pkg in lock.package {
        if pkg.loader.is_empty() {
            continue;
        }
        let base = match pkg.pkg_type.as_str() {
            "app" | "core" => app_dir.clone(),
            "sys" => format!("{}/libary", app_dir),
            "web" => format!("{}/web/{}", app_dir, pkg.name),
            "design_plugins" => format!("{}/plugins/{}", app_dir, pkg.name),
            "design_soft" => format!("{}/apps/{}", app_dir, pkg.name),
            "model" => format!("{}/models/{}", app_dir, pkg.name),
            _ => app_dir.clone(),
        };
        let missing = pkg.loader.iter().any(|entry| {
            let dest = match entry.get(1) {
                Some(v) => v,
                None => return true,
            };
            let path = if pkg.pkg_type == "app" && dest.ends_with(".html") {
                format!("{}/web/{}", app_dir, dest)
            } else {
                format!("{}/{}", base, dest)
            };
            !std::path::Path::new(&path).exists()
        });
        if missing {
            println!("  本地文件缺失: {}，标记为需要更新", pkg.name);
            let _ = remove_package_from_lock(&pkg.name);
        }
    }
}

/// 检查应用自身是否有更新
async fn app_need_update(config: &DytesConfig) -> Result<bool> {
    let (remote_info, _) = update_app::fetch_remote_app_info(&config.name).await?;

    let local = parse_version(&config.version);
    let remote = parse_version(&remote_info.version);

    if local == remote {
        return Ok(false);
    }

    if check_version(&local, &remote) {
        println!("  应用 {} 有新版本: v{} -> v{}", config.name, local, remote);
        return Ok(true);
    }

    Ok(false)
}

/// 离线模式下按本地声明的版本检查依赖
fn local_deps_need_update(deps: &std::collections::HashMap<String, String>) -> bool {
    deps.iter()
        .any(|(name, version)| need_update(name, version))
}

/// 扫描线上设计包目录，列出未在本地 dytes.json 声明的包
pub async fn discover_packages() -> Result<()> {
    let config = read_dytes_config()?;
    let groups: [(&str, &std::collections::HashMap<String, String>); 2] = [
        ("design_plugins", &config.dependencies_design_plugins),
        ("design_soft", &config.dependencies_design_soft),
    ];

    for (pkg_type, declared) in groups {
        let prefix = format!("{}/", pkg_type);
        let index_path = format!("{}/packages.json", pkg_type);

        // 优先使用 packages.json 索引
        if let Ok(text) = crate::update::utils::loader::fetch_text(&index_path).await {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(packages) = value.get("packages").and_then(|v| v.as_object()) {
                    println!("扫描线上包索引: {}", index_path);
                    for (name, version) in packages {
                        let version = version.as_str().unwrap_or("?");
                        match declared.get(name) {
                            Some(local) => {
                                println!("  [已声明] {} {}（本地 {}）", name, version, local)
                            }
                            None => {
                                println!("  [未声明] {} {}（{}）", name, version, pkg_type)
                            }
                        }
                    }
                    continue;
                }
            }
        }

        println!("扫描线上目录: {}", prefix);
        let keys = match crate::update::utils::loader::list_keys(&prefix).await {
            Ok(keys) => keys,
            Err(e) => {
                println!("  列出 {} 失败: {}", prefix, e);
                continue;
            }
        };

        for key in keys {
            if !key.ends_with("/dytes.json") {
                continue;
            }
            let parts: Vec<&str> = key.split('/').collect();
            if parts.len() != 3 {
                continue;
            }
            let name = parts[1];
            let text = match crate::update::utils::loader::fetch_text(&key).await {
                Ok(text) => text,
                Err(e) => {
                    println!("  读取 {} 失败: {}", key, e);
                    continue;
                }
            };
            let value: serde_json::Value = match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(e) => {
                    println!("  解析 {} 失败: {}", key, e);
                    continue;
                }
            };
            let version = value.get("version").and_then(|v| v.as_str()).unwrap_or("?");
            match declared.get(name) {
                Some(local) => println!("  [已声明] {} {}（本地 {}）", name, version, local),
                None => println!("  [未声明] {} {}（{}）", name, version, pkg_type),
            }
        }
    }
    Ok(())
}

/// 检查是否有更新（默认同步线上版本）
pub async fn check_updates() -> Result<bool> {
    check_updates_with(true).await
}

/// 检查是否有更新；sync_online=false 时只按本地声明版本判断
pub async fn check_updates_with(sync_online: bool) -> Result<bool> {
    prune_missing_packages();
    let config = read_dytes_config()?;

    // 应用自身：在线模式比较 app/packages.json；离线模式检查锁文件
    if sync_online {
        match app_need_update(&config).await {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(e) => println!("  检查应用自身更新失败（继续检查依赖）: {}", e),
        }
    } else if get_package_version_from_lock(&config.name)
        .ok()
        .flatten()
        .is_none()
    {
        println!("  离线模式：应用不在锁文件，需要更新");
        return Ok(true);
    }

    if sync_online {
        if deps_need_update(&config.dependencies_core, "core").await {
            return Ok(true);
        }
        if deps_need_update(&config.dependencies_sys, "sys").await {
            return Ok(true);
        }
        if deps_need_update(&config.dependencies_web, "web").await {
            return Ok(true);
        }
        if deps_need_update(&config.dependencies_design_plugins, "design_plugins").await {
            return Ok(true);
        }
        if deps_need_update(&config.dependencies_design_soft, "design_soft").await {
            return Ok(true);
        }
        if deps_need_update(&config.dependencies_model, "model").await {
            return Ok(true);
        }
    } else {
        if local_deps_need_update(&config.dependencies_core) {
            return Ok(true);
        }
        if local_deps_need_update(&config.dependencies_sys) {
            return Ok(true);
        }
        if local_deps_need_update(&config.dependencies_web) {
            return Ok(true);
        }
        if local_deps_need_update(&config.dependencies_design_plugins) {
            return Ok(true);
        }
        if local_deps_need_update(&config.dependencies_design_soft) {
            return Ok(true);
        }
        if local_deps_need_update(&config.dependencies_model) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// 单独执行已安装 Web 包的 setup.ts（用于数据库 / 路由重建）
pub fn run_init_scripts() -> Result<()> {
    let app_dir = get_app_dir();
    let web_root = std::path::Path::new(&app_dir).join("web");
    let mut scripts: Vec<(String, Vec<String>)> = Vec::new();

    if web_root.exists() {
        for entry in std::fs::read_dir(&web_root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let setup = path.join("setup.ts");
            if setup.exists() {
                println!("  发现 init 脚本: {}", setup.display());
                scripts.push((
                    path.to_string_lossy().to_string(),
                    vec!["./setup.ts".to_string()],
                ));
            }
        }
    }

    if scripts.is_empty() {
        println!("没有找到已安装 Web 包的 setup.ts");
        return Ok(());
    }

    // init 包优先执行
    scripts.sort_by(|a, b| {
        let a_init = std::path::Path::new(&a.0)
            .file_name()
            .map(|n| n == "init")
            .unwrap_or(false);
        let b_init = std::path::Path::new(&b.0)
            .file_name()
            .map(|n| n == "init")
            .unwrap_or(false);
        b_init.cmp(&a_init).then_with(|| a.0.cmp(&b.0))
    });

    run_all_init_scripts(scripts)
}

pub async fn init_dytes() -> Result<()> {
    init_dytes_with_options(None, true).await
}

pub async fn init_dytes_with_progress(progress_callback: Option<ProgressCallback>) -> Result<()> {
    init_dytes_with_options(progress_callback, true).await
}

pub async fn init_dytes_with_options(
    progress_callback: Option<ProgressCallback>,
    sync_online: bool,
) -> Result<()> {
    println!("初始化更新程序...");

    update_progress(&progress_callback, 0.0, "读取配置文件...");

    let mut config = read_dytes_config()?;
    print_config_info(&config);

    prune_missing_packages();
    let lock = read_lock().unwrap_or_default();
    println!("\n锁文件状态: {} 个包已安装", lock.package.len());

    let mut failures: Vec<String> = Vec::new();

    // 1. 先更新应用自身（应用更新后会写回新的 dytes.json，依赖列表以新配置为准）
    update_progress(&progress_callback, 5.0, "检查应用自身更新...");
    if let Err(e) =
        update_app::update_app_info(config.name.clone(), config.version.clone(), sync_online).await
    {
        println!("  应用自身更新失败: {}", e);
        failures.push(format!("应用 {}: {}", config.name, e));
    } else {
        match read_dytes_config() {
            Ok(new_config) => config = new_config,
            Err(e) => println!("  重新读取 dytes.json 失败，沿用旧配置: {}", e),
        }
    }

    let core_deps: Vec<_> = config.dependencies_core.iter().collect();
    let sys_deps: Vec<_> = config.dependencies_sys.iter().collect();
    let web_deps: Vec<_> = config.dependencies_web.iter().collect();
    let design_plugins_deps: Vec<_> = config.dependencies_design_plugins.iter().collect();
    let design_soft_deps: Vec<_> = config.dependencies_design_soft.iter().collect();
    let model_deps: Vec<_> = config.dependencies_model.iter().collect();

    let total_deps = (core_deps.len()
        + sys_deps.len()
        + web_deps.len()
        + design_plugins_deps.len()
        + design_soft_deps.len()
        + model_deps.len())
    .max(1);
    let mut completed = 0usize;

    if sync_online {
        println!("\n开始检查依赖更新（以线上版本为准）...");
    } else {
        println!("离线模式：按本地声明版本检查依赖更新...");
    }

    // 2. 核心依赖更新
    if !core_deps.is_empty() {
        update_progress(&progress_callback, 10.0, "检查核心依赖更新...");
        for (name, version) in core_deps {
            if sync_online || need_update(name, version) {
                if let Err(e) = update_os::update_os_info(name.clone(), version.clone()).await {
                    println!("  更新 {} 失败: {}", name, e);
                    failures.push(format!("core {}: {}", name, e));
                }
            } else {
                println!("  离线模式：跳过 {} (本地声明 v{})", name, version);
            }
            completed += 1;
            let progress = 10.0 + (completed as f32 / total_deps as f32) * 60.0;
            update_progress(&progress_callback, progress, &format!("更新核心: {}", name));
        }
    }

    // 3. 系统库更新
    let mut sys_init_results: Vec<update_sys::SysUpdateResult> = Vec::new();
    if !sys_deps.is_empty() {
        update_progress(&progress_callback, 70.0, "检查系统库更新...");
        for (name, version) in sys_deps {
            if sync_online || need_update(name, version) {
                match update_sys::update_sys_info(name.clone(), version.clone()).await {
                    Ok(Some(result)) => sys_init_results.push(result),
                    Ok(None) => {}
                    Err(e) => {
                        println!("  更新 {} 失败: {}", name, e);
                        failures.push(format!("sys {}: {}", name, e));
                    }
                }
            } else {
                println!("  离线模式：跳过 {} (本地声明 v{})", name, version);
            }
            completed += 1;
            let progress = 70.0 + (completed as f32 / total_deps as f32) * 15.0;
            update_progress(
                &progress_callback,
                progress,
                &format!("更新系统库: {}", name),
            );
        }
    }

    // 4. Web 资源更新 - 收集需要执行 init 的包
    let mut web_init_results = Vec::new();
    if !web_deps.is_empty() {
        update_progress(&progress_callback, 85.0, "检查 Web 资源更新...");
        for (name, version) in web_deps {
            if sync_online || need_update(name, version) {
                match update_webs::update_webs_info(name.clone(), version.clone()).await {
                    Ok(Some(result)) => {
                        web_init_results.push(result);
                    }
                    Ok(None) => {
                        println!("  {} 无需执行 init 脚本", name);
                    }
                    Err(e) => {
                        println!("  更新 {} 失败: {}", name, e);
                        failures.push(format!("web {}: {}", name, e));
                    }
                }
            } else {
                println!("  离线模式：跳过 {} (本地声明 v{})", name, version);
            }
            completed += 1;
            let progress = 85.0 + (completed as f32 / total_deps as f32) * 10.0;
            update_progress(
                &progress_callback,
                progress,
                &format!("更新 Web 资源: {}", name),
            );
        }
    }

    // 5. 设计插件更新
    if !design_plugins_deps.is_empty() {
        update_progress(&progress_callback, 95.0, "检查设计插件更新...");
        for (name, version) in design_plugins_deps {
            if sync_online || need_update(name, version) {
                if let Err(e) = update_design::update_design_info(
                    name.clone(),
                    version.clone(),
                    update_design::DesignKind::Plugin,
                )
                .await
                {
                    println!("  更新 {} 失败: {}", name, e);
                    failures.push(format!("design_plugins {}: {}", name, e));
                }
            } else {
                println!("  离线模式：跳过 {} (本地声明 v{})", name, version);
            }
            completed += 1;
            let progress = 95.0 + (completed as f32 / total_deps as f32) * 5.0;
            update_progress(
                &progress_callback,
                progress,
                &format!("更新设计插件: {}", name),
            );
        }
    }

    // 6. 设计软件更新
    if !design_soft_deps.is_empty() {
        update_progress(&progress_callback, 95.0, "检查设计软件更新...");
        for (name, version) in design_soft_deps {
            if sync_online || need_update(name, version) {
                if let Err(e) = update_design::update_design_info(
                    name.clone(),
                    version.clone(),
                    update_design::DesignKind::Soft,
                )
                .await
                {
                    println!("  更新 {} 失败: {}", name, e);
                    failures.push(format!("design_soft {}: {}", name, e));
                }
            } else {
                println!("  离线模式：跳过 {} (本地声明 v{})", name, version);
            }
            completed += 1;
            let progress = 95.0 + (completed as f32 / total_deps as f32) * 5.0;
            update_progress(
                &progress_callback,
                progress,
                &format!("更新设计软件: {}", name),
            );
        }
    }

    // 6.5 模型更新（模型是 chatllm 等组件的强制关联文件）
    if !model_deps.is_empty() {
        update_progress(&progress_callback, 95.0, "检查模型更新...");
        for (name, version) in model_deps {
            if sync_online || need_update(name, version) {
                if let Err(e) = update_model::update_model_info(name.clone(), version.clone()).await
                {
                    println!("  更新模型 {} 失败: {}", name, e);
                    failures.push(format!("model {}: {}", name, e));
                }
            } else {
                println!("  离线模式：跳过模型 {} (本地声明 v{})", name, version);
            }
            completed += 1;
            let progress = 95.0 + (completed as f32 / total_deps as f32) * 5.0;
            update_progress(&progress_callback, progress, &format!("更新模型: {}", name));
        }
    }

    // 7. sys 包与 web 包的 init 脚本统一执行（sys 先执行，负责数据库初始化）
    if !sys_init_results.is_empty() || !web_init_results.is_empty() {
        println!("\n执行 init 脚本...");

        // 排序：确保 "init" 包最先执行（顺序依赖）
        web_init_results.sort_by(|a, b| {
            if a.package_name == "init" && b.package_name != "init" {
                std::cmp::Ordering::Less
            } else if b.package_name == "init" && a.package_name != "init" {
                std::cmp::Ordering::Greater
            } else {
                a.package_name.cmp(&b.package_name)
            }
        });

        // sys 包的 setup 先执行，再做 web 包的 init
        let mut scripts_batch: Vec<(String, Vec<String>)> = sys_init_results
            .into_iter()
            .map(|r| (r.package_dir, r.init_scripts))
            .collect();
        scripts_batch.extend(
            web_init_results
                .into_iter()
                .map(|r| (r.web_dir, r.init_scripts)),
        );

        // 调用统一执行函数（内部启动/停止 cyctron）
        if let Err(e) = run_all_init_scripts(scripts_batch) {
            eprintln!("  执行 init 脚本失败: {}", e);
            failures.push(format!("init 脚本: {}", e));
        }
    }

    update_progress(
        &progress_callback,
        100.0,
        if failures.is_empty() {
            "更新完成"
        } else {
            "部分更新失败"
        },
    );
    println!("\n所有更新检查完成");

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("以下更新失败:\n  - {}", failures.join("\n  - ")))
    }
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
