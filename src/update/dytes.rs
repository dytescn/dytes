use super::update_setup::run_all_init_scripts;
use super::{update_app, update_design, update_model, update_os, update_sys, update_webs};
use crate::utils::dytes::{
    get_all_dependencies, print_config_info, read_dytes_config, Dependency, DependencyType,
    DytesConfig,
};
use crate::update::env::get_app_dir;
use crate::update::lock::{
    get_package_version_from_lock, read_lock, remove_package_from_lock,
};
use crate::utils::version::check_version;
use anyhow::{anyhow, Result};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::Path;

/// 依赖图节点：类型 + 包名。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct DepKey {
    dep_type: DependencyType,
    name: String,
}

impl DepKey {
    fn new(dep: &Dependency) -> Self {
        Self {
            dep_type: dep.dep_type,
            name: dep.name.clone(),
        }
    }

    fn display(&self) -> String {
        format!("{}/{}", self.dep_type.remote_dir(), self.name)
    }
}

/// 解析后的依赖计划：按“依赖在前”的顺序执行。
#[derive(Default)]
struct DependencyPlan {
    order: Vec<Dependency>,
    warnings: Vec<String>,
    failures: Vec<String>,
}

enum ConstraintIssue {
    Warn(String),
    Error(String),
}

fn version_constraint_issue(dep: &Dependency, remote_version: &str) -> Option<ConstraintIssue> {
    let declared = dep.version.trim();
    if declared.is_empty() || declared == "*" || declared == "latest" {
        return None;
    }
    let remote = semver::Version::parse(remote_version).ok()?;

    if let Ok(exact) = semver::Version::parse(declared) {
        if exact > remote {
            return Some(ConstraintIssue::Error(format!(
                "{} {} 声明 {}，线上只有 {}（无法满足声明版本）",
                dep.dep_type.label(),
                dep.name,
                exact,
                remote
            )));
        }
        if exact < remote {
            return Some(ConstraintIssue::Warn(format!(
                "{} {} 声明 {}，线上为 {}；按线上版本安装（锁文件会记录声明版本）",
                dep.dep_type.label(),
                dep.name,
                exact,
                remote
            )));
        }
        return None;
    }

    match semver::VersionReq::parse(declared) {
        Ok(req) if !req.matches(&remote) => Some(ConstraintIssue::Error(format!(
            "{} {} 声明范围 {}，线上版本 {} 不满足",
            dep.dep_type.label(),
            dep.name,
            declared,
            remote
        ))),
        Ok(_) => None,
        Err(_) => Some(ConstraintIssue::Warn(format!(
            "{} {} 的版本声明 '{}' 无法解析，忽略约束",
            dep.dep_type.label(),
            dep.name,
            declared
        ))),
    }
}

async fn fetch_remote_dep_manifest(dep: &Dependency) -> Result<DytesConfig> {
    let path = format!("{}/{}/dytes.json", dep.dep_type.remote_dir(), dep.name);
    let text = crate::update::loader::fetch_text(&path).await?;
    serde_json::from_str(&text).map_err(|error| anyhow!("解析 {} 失败: {}", path, error))
}

/// 展开传递依赖并做拓扑排序；环、缺失与版本约束都在这里处理。
async fn plan_dependencies(config: &DytesConfig, sync_online: bool) -> Result<DependencyPlan> {
    let roots = get_all_dependencies(config);
    if !sync_online {
        return Ok(DependencyPlan {
            order: roots,
            ..Default::default()
        });
    }

    let mut nodes: BTreeMap<DepKey, Dependency> = BTreeMap::new();
    let mut edges: BTreeMap<DepKey, Vec<DepKey>> = BTreeMap::new();
    let mut required: HashSet<DepKey> = HashSet::new();
    let mut queue: VecDeque<(Dependency, bool)> =
        roots.into_iter().map(|dep| (dep, true)).collect();
    let mut warnings = Vec::new();
    let mut failures = Vec::new();

    while let Some((dep, is_root)) = queue.pop_front() {
        let key = DepKey::new(&dep);
        if is_root {
            required.insert(key.clone());
        }
        if nodes.contains_key(&key) {
            continue;
        }

        match fetch_remote_dep_manifest(&dep).await {
            Ok(remote) => {
                if let Some(issue) = version_constraint_issue(&dep, &remote.version) {
                    match issue {
                        ConstraintIssue::Warn(message) => warnings.push(message),
                        ConstraintIssue::Error(message) => {
                            if is_root {
                                failures.push(message);
                            } else {
                                warnings.push(message);
                            }
                        }
                    }
                }

                // 非根依赖若已安装且不低于线上版本，不再展开它的子依赖：
                // 版本已满足时它的依赖也应当已安装，同时可避免无意义的环展开。
                let installed = get_package_version_from_lock(&dep.name).ok().flatten();
                let up_to_date = installed
                    .as_deref()
                    .map(|local| !check_version(local, &remote.version))
                    .unwrap_or(false);
                let children = if !is_root && up_to_date {
                    Vec::new()
                } else {
                    get_all_dependencies(&remote)
                };
                edges.insert(key.clone(), children.iter().map(DepKey::new).collect());
                nodes.insert(key, dep);
                for child in children {
                    queue.push_back((child, false));
                }
            }
            Err(error) => {
                let message = format!("{} {}: {error:#}", dep.dep_type.label(), dep.name);
                if (is_root || required.contains(&key)) && !dep.optional {
                    failures.push(message);
                } else {
                    warnings.push(format!(
                        "跳过 {}（上游未发布或不可达；声明为可选或传递依赖）",
                        message
                    ));
                }
            }
        }
    }

    let (order, cycle_warnings) = topo_sort(&nodes, &edges);
    warnings.extend(cycle_warnings);
    let mut deps = Vec::new();
    for key in order {
        if let Some(dep) = nodes.get(&key) {
            deps.push(dep.clone());
        }
    }

    Ok(DependencyPlan {
        order: deps,
        warnings,
        failures,
    })
}

/// 迭代式 DFS 拓扑排序；遇到回边（环）时跳过该边并记录警告，不中止整个计划。
///
/// 依赖图允许存在 sys ↔ web 之类的互依赖；每个包只安装一次，
/// 安装时由各 update_* 函数根据锁文件版本决定是否跳过。
fn topo_sort(
    nodes: &BTreeMap<DepKey, Dependency>,
    edges: &BTreeMap<DepKey, Vec<DepKey>>,
) -> (Vec<DepKey>, Vec<String>) {
    let mut color: BTreeMap<DepKey, u8> = nodes.keys().cloned().map(|key| (key, 0)).collect();
    let mut order = Vec::new();
    let mut warnings = Vec::new();
    let mut seen_cycles: HashSet<String> = HashSet::new();

    for start in nodes.keys() {
        if color.get(start).copied().unwrap_or(0) != 0 {
            continue;
        }
        let mut stack: Vec<(DepKey, usize)> = vec![(start.clone(), 0)];
        color.insert(start.clone(), 1);

        while let Some((key, index)) = stack.last().cloned() {
            let children = edges.get(&key).cloned().unwrap_or_default();
            if index < children.len() {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
                let child = children[index].clone();
                match color.get(&child).copied().unwrap_or(0) {
                    0 => {
                        color.insert(child.clone(), 1);
                        stack.push((child, 0));
                    }
                    1 => {
                        // 回边：跳过这条边，继续完成其余节点。
                        let chain = stack
                            .iter()
                            .map(|(node, _)| node.display())
                            .chain(std::iter::once(child.display()))
                            .collect::<Vec<_>>()
                            .join(" -> ");
                        if seen_cycles.insert(chain.clone()) {
                            warnings.push(format!("检测到依赖环，已跳过环上的边并继续: {chain}"));
                        }
                    }
                    _ => {}
                }
            } else {
                color.insert(key.clone(), 2);
                order.push(key);
                stack.pop();
            }
        }
    }

    (order, warnings)
}

/// 按依赖类型调用对应的更新函数。
async fn update_one_dependency(
    dep: &Dependency,
    sys_init_results: &mut Vec<update_sys::SysUpdateResult>,
    web_init_results: &mut Vec<update_webs::WebUpdateResult>,
) -> Result<()> {
    match dep.dep_type {
        DependencyType::Core => {
            update_os::update_os_info(dep.name.clone(), dep.version.clone()).await
        }
        DependencyType::Sys => {
            if let Some(result) =
                update_sys::update_sys_info(dep.name.clone(), dep.version.clone()).await?
            {
                sys_init_results.push(result);
            }
            Ok(())
        }
        DependencyType::Web => {
            if let Some(result) =
                update_webs::update_webs_info(dep.name.clone(), dep.version.clone()).await?
            {
                web_init_results.push(result);
            }
            Ok(())
        }
        DependencyType::DesignPlugins => {
            update_design::update_design_info(
                dep.name.clone(),
                dep.version.clone(),
                update_design::DesignKind::Plugin,
                false,
                None,
            )
            .await?;
            Ok(())
        }
        DependencyType::DesignSoft => {
            update_design::update_design_info(
                dep.name.clone(),
                dep.version.clone(),
                update_design::DesignKind::Soft,
                false,
                None,
            )
            .await?;
            Ok(())
        }
        DependencyType::Model => {
            update_model::update_model_info(dep.name.clone(), dep.version.clone()).await
        }
    }
}

/// 为一个设计包 / 插件安装其声明的依赖。
///
/// 规则与 `dytes update` 一致：
/// - 已安装且版本不低于线上版本：跳过；
/// - 未安装或版本落后：安装线上最新版本；
/// - 依赖图存在环：跳过回边并继续，不中止安装。
pub(crate) async fn install_dependencies_for(config: &DytesConfig) -> Result<()> {
    let plan = plan_dependencies(config, true).await?;
    for warning in &plan.warnings {
        println!("  警告: {warning}");
    }
    if !plan.failures.is_empty() {
        return Err(anyhow!("依赖检查失败: {}", plan.failures.join("; ")));
    }

    let mut sys_init_results = Vec::new();
    let mut web_init_results = Vec::new();
    for dep in plan.order {
        if dep.name == config.name {
            continue;
        }
        update_one_dependency(&dep, &mut sys_init_results, &mut web_init_results).await?;
    }

    let mut scripts: Vec<(String, Vec<String>)> = Vec::new();
    scripts.extend(
        sys_init_results
            .into_iter()
            .map(|result| (result.package_dir, result.init_scripts)),
    );
    scripts.extend(
        web_init_results
            .into_iter()
            .map(|result| (result.web_dir, result.init_scripts)),
    );
    scripts.retain(|(_, init_scripts)| !init_scripts.is_empty());
    if !scripts.is_empty() {
        run_all_init_scripts(scripts).await?;
    }
    Ok(())
}

/// 构造只包含目标包的临时清单，避免单包安装读取应用的其他依赖。
fn single_dependency_config(
    dep_type: DependencyType,
    name: &str,
    version: &str,
) -> Result<DytesConfig> {
    let key = match dep_type {
        DependencyType::Core => "dependencies-core",
        DependencyType::Sys => "dependencies-sys",
        DependencyType::Web => "dependencies-web",
        DependencyType::DesignPlugins => "dependencies-design-plugins",
        DependencyType::DesignSoft => "dependencies-design-soft",
        DependencyType::Model => "dependencies-model",
    };
    let mut value = serde_json::json!({
        "name": "__single_install__",
        "version": "0.0.0",
        "type": dep_type.as_str(),
    });
    value[key] = serde_json::json!({ name: version });
    serde_json::from_value(value).map_err(|error| anyhow!("构造单包安装配置失败: {error}"))
}

/// 安装单个包及其必要依赖，不检查 / 更新应用清单中的其他依赖。
pub async fn install_one_dependency(
    dep_type: DependencyType,
    name: &str,
    version: &str,
    sync_online: bool,
) -> Result<()> {
    let config = single_dependency_config(dep_type, name, version)?;
    let plan = plan_dependencies(&config, sync_online).await?;
    for warning in &plan.warnings {
        println!("  警告: {warning}");
    }
    if !plan.failures.is_empty() {
        return Err(anyhow!("依赖检查失败: {}", plan.failures.join("; ")));
    }

    let mut sys_init_results = Vec::new();
    let mut web_init_results = Vec::new();
    for dep in plan.order {
        update_one_dependency(&dep, &mut sys_init_results, &mut web_init_results).await?;
    }

    let mut scripts: Vec<(String, Vec<String>)> = Vec::new();
    scripts.extend(
        sys_init_results
            .into_iter()
            .map(|result| (result.package_dir, result.init_scripts)),
    );
    scripts.extend(
        web_init_results
            .into_iter()
            .map(|result| (result.web_dir, result.init_scripts)),
    );
    scripts.retain(|(_, init_scripts)| !init_scripts.is_empty());
    if !scripts.is_empty() {
        run_all_init_scripts(scripts).await?;
    }
    Ok(())
}

/// 进度回调类型
pub type ProgressCallback = Box<dyn Fn(f32, &str) + Send + Sync>;

/// 解析版本号，去除 ^ 或 ~ 前缀
fn parse_version(version: &str) -> String {
    version
        .trim_start_matches('^')
        .trim_start_matches('~')
        .to_string()
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

/// 从 packages.json 的 `packages` 对象中取某包的版本。
///
/// 兼容两种索引：
/// - 旧：`"name": "1.0.0"`
/// - 新：`"<uuid>": {"name": "...", "version": "...", ...}`
fn package_version(packages: &serde_json::Value, name: &str) -> Option<String> {
    let map = packages.as_object()?;
    if let Some(value) = map.get(name) {
        if let Some(version) = value
            .as_str()
            .or_else(|| value.get("version").and_then(|v| v.as_str()))
        {
            return Some(version.to_string());
        }
    }
    map.values()
        .find(|value| value.get("name").and_then(|v| v.as_str()) == Some(name))
        .and_then(|value| value.get("version").and_then(|v| v.as_str()))
        .map(str::to_string)
}

/// 列出 packages.json 中的 `(name, version)`，兼容新旧两种索引。
fn package_versions(packages: &serde_json::Value) -> Vec<(String, String)> {
    let Some(map) = packages.as_object() else {
        return Vec::new();
    };
    let mut result: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (key, value) in map {
        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(key)
            .to_string();
        let version = value
            .as_str()
            .or_else(|| value.get("version").and_then(|v| v.as_str()))
            .unwrap_or("?")
            .to_string();
        result.insert(name, version);
    }
    result.into_iter().collect()
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
    if let Ok(text) = crate::update::loader::fetch_text(&index_path).await {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(packages) = value.get("packages") {
                if let Some(version) = package_version(packages, name) {
                    return Ok(version);
                }
            }
        }
    }

    // 回退到包自身的 dytes.json
    let path = format!("{}/{}/dytes.json", dir, name);
    let text = crate::update::loader::fetch_text(&path).await?;
    let config: DytesConfig = serde_json::from_str(&text)?;
    Ok(config.version)
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
        if let Ok(text) = crate::update::loader::fetch_text(&index_path).await {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(packages) = value.get("packages").and_then(|v| v.as_object()) {
                    println!("扫描线上包索引: {}", index_path);
                    for (name, version) in
                        package_versions(&serde_json::Value::Object(packages.clone()))
                    {
                        match declared.get(&name) {
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
        let keys = match crate::update::loader::list_keys(&prefix).await {
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
            let text = match crate::update::loader::fetch_text(&key).await {
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

/// 启动更新的节流窗口：1 天。
const STARTUP_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// `dytes.json` 中记录下次更新检测时间的字段（Unix 秒）。
pub const UPDATE_CHECK_AFTER: &str = "update_check_after";

/// 两次更新检测之间的最短间隔。
pub const UPDATE_CHECK_INTERVAL_SECS: u64 = STARTUP_INTERVAL_SECS;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// 读取 `dytes.json` 中的下次更新检测时间。
pub fn update_check_after(path: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get(UPDATE_CHECK_AFTER)?.as_u64()
}

/// 把下次更新检测时间写入 `dytes.json`（Unix 秒）。
pub fn set_update_check_after(dir: &Path, timestamp: u64) -> Result<()> {
    let path = dir.join("dytes.json");
    let text = std::fs::read_to_string(&path)?;
    let mut value: serde_json::Value = serde_json::from_str(&text)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("dytes.json 不是 JSON 对象"))?;
    object.insert(
        UPDATE_CHECK_AFTER.to_string(),
        serde_json::json!(timestamp),
    );
    std::fs::write(&path, serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

/// 判断启动更新是否应执行：遵守 `auto_update`，并在 1 天内跳过。
pub fn startup_allowed() -> Result<bool> {
    let config = read_dytes_config()?;
    if !config.auto_update {
        println!("  auto_update=false，跳过启动更新");
        return Ok(false);
    }

    let now = now_secs();
    if let Some(after) = update_check_after(Path::new("dytes.json")) {
        if now < after {
            println!("  下次更新检测时间未到，跳过更新");
            return Ok(false);
        }
    }

    if let Ok(text) = std::fs::read_to_string(".update_check") {
        if let Ok(value) = text.trim().parse::<u64>() {
            if now.saturating_sub(value) < STARTUP_INTERVAL_SECS {
                println!("  最近已检查，跳过更新");
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// 记录本次启动检查时间。
pub fn mark_startup_checked() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let _ = std::fs::write(".update_check", now.to_string());
}

/// 更新检查报告。
pub struct CheckReport {
    pub has_updates: bool,
    pub value: serde_json::Value,
}

/// 规范化包类型别名。
fn normalize_pkg_type(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "plugin" | "plugins" | "design_plugins" | "design-plugins" => "design_plugins".to_string(),
        "design_soft" | "design-soft" | "soft" => "design_soft".to_string(),
        "system" => "sys".to_string(),
        "webs" => "web".to_string(),
        "models" => "model".to_string(),
        "apps" => "app".to_string(),
        other => other.to_string(),
    }
}

/// 检查更新并返回结构化结果。
///
/// `types` 为空时检查全部类型；指定类型时只检查该类型。检查范围包括
/// `dytes.json` 声明的依赖，以及 `dytes.lock` 中已安装的同类型包，
/// 因此未在清单中声明的插件 / 软件也能被发现。
pub async fn check_report(sync_online: bool, types: Option<&[String]>) -> Result<CheckReport> {
    use serde_json::json;

    prune_missing_packages();
    let config = read_dytes_config()?;
    let lock = read_lock().unwrap_or_default();
    let local_version = |name: &str| -> Option<String> {
        lock.package
            .iter()
            .find(|package| package.name == name)
            .map(|package| package.version.clone())
    };

    let selected = |pkg_type: &str| -> bool {
        match types {
            Some(list) if !list.is_empty() => {
                list.iter().any(|item| normalize_pkg_type(item) == pkg_type)
            }
            _ => true,
        }
    };

    let mut updates: Vec<serde_json::Value> = Vec::new();

    if selected("app") {
        let local = local_version(&config.name);
        if sync_online {
            match app_need_update(&config).await {
                Ok(needs) => updates.push(json!({
                    "type": "app",
                    "name": config.name,
                    "local_version": local,
                    "remote_version": serde_json::Value::Null,
                    "needs_update": needs,
                })),
                Err(error) => println!("  检查应用自身更新失败（继续检查依赖）: {}", error),
            }
        } else {
            updates.push(json!({
                "type": "app",
                "name": config.name,
                "local_version": local,
                "remote_version": serde_json::Value::Null,
                "needs_update": local.is_none(),
            }));
        }
    }

    let groups: [(&str, &std::collections::HashMap<String, String>); 6] = [
        ("core", &config.dependencies_core),
        ("sys", &config.dependencies_sys),
        ("web", &config.dependencies_web),
        ("design_plugins", &config.dependencies_design_plugins),
        ("design_soft", &config.dependencies_design_soft),
        ("model", &config.dependencies_model),
    ];

    for (pkg_type, deps) in groups {
        if !selected(pkg_type) {
            continue;
        }

        let mut names: std::collections::BTreeSet<String> = deps.keys().cloned().collect();
        for package in &lock.package {
            if package.pkg_type == pkg_type {
                names.insert(package.name.clone());
            }
        }

        for name in names {
            let local = local_version(&name);
            let remote = if sync_online {
                match remote_version(pkg_type, &name).await {
                    Ok(version) => Some(version),
                    Err(error) => {
                        println!("  检查 {} 线上版本失败: {}", name, error);
                        None
                    }
                }
            } else {
                deps.get(&name).cloned().or_else(|| local.clone())
            };

            let needs_update = match (&local, &remote) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(local), Some(remote)) => match (
                    semver::Version::parse(&parse_version(local)),
                    semver::Version::parse(&parse_version(remote)),
                ) {
                    (Ok(local), Ok(remote)) => remote > local,
                    _ => parse_version(local) != parse_version(remote),
                },
            };

            updates.push(json!({
                "type": pkg_type,
                "name": name,
                "local_version": local,
                "remote_version": remote,
                "needs_update": needs_update,
            }));
        }
    }

    // 展开传递依赖：把“已声明但不在本地清单 / 锁文件”的包也列出来，
    // 便于在安装前发现 chatllm 之类的依赖缺口。
    if sync_online {
        match plan_dependencies(&config, true).await {
            Ok(plan) => {
                for dep in plan.order {
                    if !selected(dep.dep_type.as_str()) {
                        continue;
                    }
                    let name = dep.name.clone();
                    let pkg_type = dep.dep_type.as_str();
                    if updates.iter().any(|item| {
                        item.get("type").and_then(|v| v.as_str()) == Some(pkg_type)
                            && item.get("name").and_then(|v| v.as_str()) == Some(name.as_str())
                    }) {
                        continue;
                    }
                    let local = local_version(&name);
                    updates.push(json!({
                        "type": pkg_type,
                        "name": name,
                        "local_version": local,
                        "remote_version": serde_json::Value::Null,
                        "declared": dep.version,
                        "needs_update": local.is_none(),
                        "source": "transitive",
                    }));
                }
            }
            Err(error) => println!("  展开传递依赖失败（继续检查已声明依赖）: {error:#}"),
        }
    }

    let count = updates
        .iter()
        .filter(|item| {
            item.get("needs_update")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        })
        .count();

    let value = json!({
        "check": true,
        "types": types.unwrap_or(&[]),
        "count": count,
        "updates": updates,
    });

    Ok(CheckReport {
        has_updates: count > 0,
        value,
    })
}

/// 检查是否有更新（默认同步线上版本）
pub async fn check_updates() -> Result<bool> {
    check_updates_with(true).await
}

/// 检查是否有更新；sync_online=false 时只按本地声明版本判断
pub async fn check_updates_with(sync_online: bool) -> Result<bool> {
    Ok(check_report(sync_online, None).await?.has_updates)
}

/// 单独执行已安装 Web 包的 setup.json（用于数据库 / 路由重建）
pub async fn run_init_scripts() -> Result<()> {
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
            let setup = path.join("setup.json");
            if setup.exists() {
                println!("  发现 init 脚本: {}", setup.display());
                scripts.push((
                    path.to_string_lossy().to_string(),
                    vec!["./setup.json".to_string()],
                ));
            }
        }
    }

    if scripts.is_empty() {
        println!("没有找到已安装 Web 包的 setup.json");
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

    run_all_init_scripts(scripts).await
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
        if is_missing_remote_manifest(&e) {
            println!("  应用自身尚未发布，跳过应用更新: {e}");
        } else {
            println!("  应用自身更新失败: {}", e);
            failures.push(format!("应用 {}: {}", config.name, e));
        }
    } else {
        match read_dytes_config() {
            Ok(new_config) => config = new_config,
            Err(e) => println!("  重新读取 dytes.json 失败，沿用旧配置: {}", e),
        }
    }

    let plan = match plan_dependencies(&config, sync_online).await {
        Ok(plan) => plan,
        Err(error) => {
            println!("  依赖解析失败: {error:#}");
            failures.push(format!("依赖解析: {error:#}"));
            DependencyPlan::default()
        }
    };
    for warning in &plan.warnings {
        println!("  警告: {}", warning);
    }
    failures.extend(plan.failures);

    let total_deps = plan.order.len().max(1);
    let mut completed = 0usize;

    if sync_online {
        println!(
            "
开始检查依赖更新（以线上版本为准；声明版本用于约束校验）..."
        );
    } else {
        println!("离线模式：按本地声明版本检查依赖更新...");
    }

    let mut sys_init_results: Vec<update_sys::SysUpdateResult> = Vec::new();
    let mut web_init_results: Vec<update_webs::WebUpdateResult> = Vec::new();

    for dep in &plan.order {
        if !sync_online && !need_update(&dep.name, &dep.version) {
            println!("  离线模式：跳过 {} (本地声明 v{})", dep.name, dep.version);
            completed += 1;
            continue;
        }

        if let Err(error) =
            update_one_dependency(dep, &mut sys_init_results, &mut web_init_results).await
        {
            println!("  更新 {} 失败: {error:#}", dep.name);
            failures.push(format!("{} {}: {error:#}", dep.dep_type.as_str(), dep.name));
        }

        completed += 1;
        let progress = 10.0 + (completed as f32 / total_deps as f32) * 85.0;
        update_progress(
            &progress_callback,
            progress,
            &format!("更新{}: {}", dep.dep_type.label(), dep.name),
        );
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
        if let Err(e) = run_all_init_scripts(scripts_batch).await {
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

/// 远程清单不存在（尚未发布）时不应作为更新失败。
fn is_missing_remote_manifest(error: &anyhow::Error) -> bool {
    let text = format!("{error:#}").to_lowercase();
    text.contains("nosuchkey")
        || text.contains("not found")
        || text.contains("notfound")
        || text.contains("404")
        || text.contains("下载 app/")
}

fn update_progress(callback: &Option<ProgressCallback>, progress: f32, status: &str) {
    if let Some(cb) = callback {
        cb(progress, status);
    }
    println!("进度: {:.1}% - {}", progress, status);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(dep_type: DependencyType, name: &str, version: &str) -> Dependency {
        Dependency::new(name, version, dep_type)
    }

    fn key(dep: &Dependency) -> DepKey {
        DepKey::new(dep)
    }

    #[test]
    fn topo_sort_puts_dependencies_first() {
        let chatllm = dep(DependencyType::Sys, "chatllm", "1.0.9");
        let database = dep(DependencyType::Sys, "database", "1.0.2");
        let info = dep(DependencyType::Sys, "info_lib", "0.1.0");

        let mut nodes = BTreeMap::new();
        for item in [&chatllm, &database, &info] {
            nodes.insert(key(item), item.clone());
        }
        let mut edges = BTreeMap::new();
        edges.insert(key(&chatllm), vec![key(&database), key(&info)]);
        edges.insert(key(&database), vec![]);
        edges.insert(key(&info), vec![key(&database)]);

        let (order, warnings) = topo_sort(&nodes, &edges);
        assert!(warnings.is_empty());
        let position = |name: &str| {
            order
                .iter()
                .position(|item| item.name == name)
                .expect("missing node")
        };
        assert!(position("database") < position("info_lib"));
        assert!(position("info_lib") < position("chatllm"));
    }

    #[test]
    fn topo_sort_tolerates_cycle() {
        let a = dep(DependencyType::Sys, "a", "1.0.0");
        let b = dep(DependencyType::Sys, "b", "1.0.0");
        let mut nodes = BTreeMap::new();
        nodes.insert(key(&a), a.clone());
        nodes.insert(key(&b), b.clone());
        let mut edges = BTreeMap::new();
        edges.insert(key(&a), vec![key(&b)]);
        edges.insert(key(&b), vec![key(&a)]);

        let (order, warnings) = topo_sort(&nodes, &edges);
        assert_eq!(order.len(), 2);
        assert!(warnings.iter().any(|item| item.contains("依赖环")));
        assert!(warnings.iter().any(|item| item.contains("system/a")));
        assert!(warnings.iter().any(|item| item.contains("system/b")));
    }

    #[test]
    fn version_constraints_report_mismatch() {
        let range = dep(DependencyType::Sys, "info_lib", "^0.2.0");
        assert!(matches!(
            version_constraint_issue(&range, "0.1.1"),
            Some(ConstraintIssue::Error(_))
        ));

        let exact_old = dep(DependencyType::Sys, "info_lib", "0.1.0");
        assert!(matches!(
            version_constraint_issue(&exact_old, "0.1.1"),
            Some(ConstraintIssue::Warn(_))
        ));

        let exact_new = dep(DependencyType::Sys, "info_lib", "0.2.0");
        assert!(matches!(
            version_constraint_issue(&exact_new, "0.1.1"),
            Some(ConstraintIssue::Error(_))
        ));
    }

    #[test]
    fn single_dependency_config_contains_only_target() {
        let config = single_dependency_config(DependencyType::Sys, "cloud_lib", "1.0.0").unwrap();
        assert_eq!(config.dependencies_sys.len(), 1);
        assert_eq!(
            config.dependencies_sys.get("cloud_lib").map(String::as_str),
            Some("1.0.0")
        );
        assert!(config.dependencies_core.is_empty());
        assert!(config.dependencies_web.is_empty());
        assert!(config.dependencies_model.is_empty());
    }

    #[test]
    fn update_check_after_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dytes.json");
        std::fs::write(&path, r#"{"name":"x","version":"1.0.0","type":"app"}"#).unwrap();
        assert_eq!(update_check_after(&path), None);

        set_update_check_after(dir.path(), 12345).unwrap();
        assert_eq!(update_check_after(&path), Some(12345));
    }
}
