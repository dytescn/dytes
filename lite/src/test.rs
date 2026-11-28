//! `dytes test`：本地包自检、测试环境管理与离线安装演练。
//!
//! 流程：
//! 1. 在包目录创建 `.dytes/`（并确保它已写入 `.gitignore`）；
//! 2. 复用 `dytes check` 的校验逻辑；
//! 3. 把 `test-dependency` 声明的、本地已安装的依赖复制到 `.dytes/env/`；
//! 4. 在 `.dytes/` 内按安装布局演练当前包的落盘与 `dytes.lock` 生成。
//!
//! 全程不访问网络，不修改真实安装目录。

use clap::Args;
use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::publish::cli::{evaluate_check, CheckOutcome};
use crate::update::lock::{DytesLock, PackageLock};
use crate::utils::checksum::verify_file;
use crate::utils::dytes::{DependencyType, DytesConfig, PackageType};
use crate::utils::error::{DytesError, Result};
use crate::utils::platform;
use crate::utils::plugin_manifest::CheckReport;

#[derive(Args)]
pub struct TestArgs {
    /// 包目录（默认当前目录）
    #[arg(value_name = "DIR")]
    dir: Option<PathBuf>,
    /// 只接受指定类型；与清单 type 不一致时报错
    #[arg(long = "type", value_name = "TYPE")]
    pkg_type: Option<String>,
    /// 按指定平台选择 platform-exports（默认当前平台）
    #[arg(long)]
    platform: Option<String>,
    /// 允许没有 exports 的“仅清单”包
    #[arg(long = "manifest-only")]
    manifest_only: bool,
    /// 演练目录，相对 `.dytes/`（默认自动生成）；必须不存在
    #[arg(long = "work-dir")]
    work_dir: Option<PathBuf>,
    /// 保留演练目录，便于排查
    #[arg(long)]
    keep: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy)]
enum GitignoreAction {
    Created,
    Updated,
    Unchanged,
}

impl GitignoreAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
        }
    }
}

struct TestSetup {
    dytes_dir: PathBuf,
    env_dir: PathBuf,
    gitignore: GitignoreAction,
}

struct Rehearsal {
    work_dir: PathBuf,
    kept: bool,
    files: Vec<serde_json::Value>,
}

pub fn run(args: TestArgs) -> Result<()> {
    let dir = args.dir.unwrap_or(std::env::current_dir()?);
    // 依赖安装期间会临时切换工作目录，先转成绝对路径避免后续相对路径失效。
    let dir = if dir.is_absolute() {
        dir
    } else {
        std::env::current_dir()?.join(dir)
    };
    let setup = setup_test_root(&dir)?;
    let platform = args.platform.unwrap_or_else(platform::current_platform);
    let outcome = evaluate_check(
        &dir,
        args.pkg_type.as_deref(),
        &platform,
        args.manifest_only,
    )?;

    let mut report = outcome.report.clone();
    report.pass("测试目录已就绪", setup.dytes_dir.display().to_string());
    let gitignore_detail = dir.join(".gitignore").display().to_string();
    match setup.gitignore {
        GitignoreAction::Created => {
            report.pass(".gitignore 已创建并忽略 .dytes/", gitignore_detail)
        }
        GitignoreAction::Updated => report.pass(".gitignore 已追加 .dytes/", gitignore_detail),
        GitignoreAction::Unchanged => report.pass(".gitignore 已忽略 .dytes/", gitignore_detail),
    }

    let mut rehearsal = None;
    let mut dependencies = Vec::new();
    let mut failure_code = None;

    if report.passed() {
        match install_test_dependencies(&outcome.config, &setup.env_dir, &mut report) {
            Ok(installed) => {
                dependencies = installed;
                rehearsal = Some(rehearse(
                    &dir,
                    &outcome,
                    args.manifest_only,
                    args.keep,
                    args.work_dir,
                    &setup,
                    &mut report,
                )?);
            }
            Err(error) => {
                report.fail("依赖安装", error.message.clone());
                failure_code = Some(error.code);
            }
        }
    }

    if args.json {
        let checks: Vec<serde_json::Value> = report
            .items
            .iter()
            .map(|item| {
                json!({
                    "label": item.label,
                    "ok": item.ok,
                    "detail": item.detail,
                    "severity": item.severity(),
                })
            })
            .collect();
        let mut value = json!({
            "passed": report.passed(),
            "dir": dir,
            "name": outcome.config.name,
            "type": outcome.config.package_type()?.as_str(),
            "version": outcome.config.version,
            "uuid": outcome.config.uuid,
            "platform": outcome.platform,
            "manifest_only": args.manifest_only,
            "dytes_dir": setup.dytes_dir,
            "env_dir": setup.env_dir,
            "gitignore": setup.gitignore.as_str(),
            "dependencies": dependencies,
            "checks": checks,
        });
        if let Some(rehearsal) = &rehearsal {
            value["work_dir"] = json!(rehearsal.work_dir);
            value["kept"] = json!(rehearsal.kept);
            value["files"] = json!(rehearsal.files);
        }
        println!("{value}");
    } else {
        report.print();
        println!("测试目录: {}", setup.dytes_dir.display());
        println!("依赖目录: {}", setup.env_dir.display());
        if let Some(rehearsal) = &rehearsal {
            println!("演练目录: {}", rehearsal.work_dir.display());
            if rehearsal.kept {
                println!("已保留演练目录（--keep）");
            }
        }
    }

    if report.passed() {
        Ok(())
    } else {
        Err(DytesError::new(
            failure_code.unwrap_or(3),
            format!("包自检失败: {} 项", report.failure_count()),
        ))
    }
}

/// 创建或刷新测试目录，并确保 `.dytes/` 被 `.gitignore` 忽略。
fn setup_test_root(package_dir: &Path) -> Result<TestSetup> {
    let dytes_dir = package_dir.join(".dytes");
    fs::create_dir_all(&dytes_dir)
        .map_err(|e| DytesError::failed(format!("创建 .dytes 目录失败: {e}")))?;

    // 每次测试重新生成依赖环境，避免旧依赖残留影响结果。
    let env_dir = dytes_dir.join("env");
    if env_dir.exists() {
        fs::remove_dir_all(&env_dir)
            .map_err(|e| DytesError::failed(format!("清理 .dytes/env 失败: {e}")))?;
    }
    fs::create_dir_all(&env_dir)
        .map_err(|e| DytesError::failed(format!("创建 .dytes/env 失败: {e}")))?;

    let gitignore = ensure_gitignore(package_dir)?;
    Ok(TestSetup {
        dytes_dir,
        env_dir,
        gitignore,
    })
}

fn ensure_gitignore(package_dir: &Path) -> Result<GitignoreAction> {
    let path = package_dir.join(".gitignore");
    if !path.exists() {
        fs::write(&path, "# dytes 测试环境\n.dytes/\n")
            .map_err(|e| DytesError::failed(format!("写入 {} 失败: {e}", path.display())))?;
        return Ok(GitignoreAction::Created);
    }

    let text = fs::read_to_string(&path)
        .map_err(|e| DytesError::failed(format!("读取 {} 失败: {e}", path.display())))?;
    if text.lines().any(is_dytes_ignore_line) {
        return Ok(GitignoreAction::Unchanged);
    }

    let mut updated = text;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    if !updated.is_empty() && !updated.ends_with("\n\n") {
        updated.push('\n');
    }
    updated.push_str("# dytes 测试环境\n.dytes/\n");
    fs::write(&path, updated)
        .map_err(|e| DytesError::failed(format!("写入 {} 失败: {e}", path.display())))?;
    Ok(GitignoreAction::Updated)
}

fn is_dytes_ignore_line(line: &str) -> bool {
    matches!(line.trim(), ".dytes/" | ".dytes" | "/.dytes/" | "/.dytes")
}

/// 把 `test-dependency` 合并进清单的 `dependencies-*`。
///
/// 清单声明是基础，`test-dependency` 作为测试专用补充；同名条目以
/// `test-dependency` 为准。
pub fn merge_test_dependencies(config: &DytesConfig) -> Result<DytesConfig> {
    let mut merged = config.clone();
    for (type_name, deps) in &config.test_dependency {
        let dep_type = DependencyType::from_name(type_name).ok_or_else(|| {
            DytesError::config(format!("test-dependency 包含未知包类型: {type_name}"))
        })?;
        let target = match dep_type {
            DependencyType::Core => &mut merged.dependencies_core,
            DependencyType::Sys => &mut merged.dependencies_sys,
            DependencyType::Web => &mut merged.dependencies_web,
            DependencyType::DesignPlugins => &mut merged.dependencies_design_plugins,
            DependencyType::DesignSoft => &mut merged.dependencies_design_soft,
            DependencyType::Model => &mut merged.dependencies_model,
        };
        for (name, version) in deps {
            target.insert(name.clone(), version.clone());
        }
    }
    Ok(merged)
}

fn has_dependencies(config: &DytesConfig) -> bool {
    !config.dependencies_core.is_empty()
        || !config.dependencies_sys.is_empty()
        || !config.dependencies_web.is_empty()
        || !config.dependencies_design_plugins.is_empty()
        || !config.dependencies_design_soft.is_empty()
        || !config.dependencies_model.is_empty()
}

/// 在线更新下载当前包的依赖到 `.dytes/env/`。
///
/// 当前包自身不下载；`test-dependency` 合并进清单后，与 `dependencies-*`
/// 一起交给更新引擎按声明 semver 安装线上最新版本。
fn install_test_dependencies(
    config: &DytesConfig,
    env_dir: &Path,
    report: &mut CheckReport,
) -> Result<Vec<serde_json::Value>> {
    let merged = merge_test_dependencies(config)?;
    if !has_dependencies(&merged) {
        return Ok(Vec::new());
    }

    // `.dytes/env/` 作为测试安装根；清单供更新引擎读取与排查。
    let manifest = serde_json::to_string_pretty(&merged)
        .map_err(|e| DytesError::failed(format!("序列化测试依赖清单失败: {e}")))?;
    let manifest_path = env_dir.join("dytes.json");
    fs::write(&manifest_path, manifest)
        .map_err(|e| DytesError::failed(format!("写入 {} 失败: {e}", manifest_path.display())))?;
    report.pass("测试依赖清单已就绪", manifest_path.display().to_string());

    run_dependency_install(env_dir, &merged)?;

    let installed = read_installed_dependencies(env_dir);
    if installed.is_empty() {
        report.fail("测试依赖安装", "更新引擎未写入 dytes.lock");
        return Err(DytesError::failed("测试依赖安装未产生锁文件记录"));
    }
    for dep in &installed {
        report.pass(
            format!(
                "测试依赖 {}/{}",
                dep["type"].as_str().unwrap_or(""),
                dep["name"].as_str().unwrap_or("")
            ),
            dep["version"].as_str().unwrap_or("").to_string(),
        );
    }
    Ok(installed)
}

/// 去掉 Windows canonicalize 产生的 `\\?\` 前缀，避免后续字符串拼接 `/` 失败。
fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().into_owned();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

/// 以 `.dytes/env/` 为安装根运行更新引擎。
///
/// 更新引擎的锁文件使用相对路径，因此临时切换工作目录并设置
/// `DYTES_APP_DIR`，结束后无论如何都恢复进程状态。
fn run_dependency_install(env_dir: &Path, config: &DytesConfig) -> Result<()> {
    let env_dir = env_dir.canonicalize().map_err(|e| {
        DytesError::failed(format!("解析测试环境目录失败: {}: {e}", env_dir.display()))
    })?;
    // Windows 的 canonicalize 会返回 `\\?\` 前缀；后续更新引擎用
    // `format!("{app_dir}/libary")` 拼接正斜杠，verbatim 路径不接受 `/`，
    // 会返回 os error 123（文件名、目录名或卷标语法不正确）。
    let env_dir = strip_verbatim_prefix(env_dir);
    let previous_dir = std::env::current_dir()
        .map_err(|e| DytesError::failed(format!("读取当前目录失败: {e}")))?;
    let previous_app_dir = std::env::var_os("DYTES_APP_DIR");

    std::env::set_current_dir(&env_dir)
        .map_err(|e| DytesError::failed(format!("切换测试环境目录失败: {e}")))?;
    std::env::set_var("DYTES_APP_DIR", &env_dir);

    let config = config.clone();
    let joined = std::thread::Builder::new()
        .name("dytes-test-deps".to_string())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || -> std::result::Result<(), String> {
            let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
            runtime
                .block_on(crate::update::dytes::install_dependencies_only(&config))
                .map_err(|e| format!("{e:#}"))
        })
        .map_err(|e| DytesError::failed(format!("创建依赖安装线程失败: {e}")))?
        .join();

    // 无论成功失败都恢复进程状态，避免影响后续演练。
    let _ = std::env::set_current_dir(&previous_dir);
    match previous_app_dir {
        Some(value) => std::env::set_var("DYTES_APP_DIR", value),
        None => std::env::remove_var("DYTES_APP_DIR"),
    }

    match joined {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(DytesError::failed(format!("依赖下载失败: {error}"))),
        Err(_) => Err(DytesError::failed("依赖安装线程异常退出")),
    }
}

fn read_installed_dependencies(env_dir: &Path) -> Vec<serde_json::Value> {
    let Ok(text) = fs::read_to_string(env_dir.join("dytes.lock")) else {
        return Vec::new();
    };
    let Ok(lock) = serde_json::from_str::<DytesLock>(&text) else {
        return Vec::new();
    };
    lock.package
        .into_iter()
        .map(|package| {
            json!({
                "type": package.pkg_type,
                "name": package.name,
                "version": package.version,
                "declared": package.declared,
                "target": env_dir,
            })
        })
        .collect()
}

/// 在 `.dytes/` 中模拟安装：复制导出文件、init 脚本并写入锁文件。
fn rehearse(
    dir: &Path,
    outcome: &CheckOutcome,
    manifest_only: bool,
    keep: bool,
    explicit_work_dir: Option<PathBuf>,
    setup: &TestSetup,
    report: &mut CheckReport,
) -> Result<Rehearsal> {
    let pkg_type = outcome.config.package_type()?;
    if !is_safe_install_name(&outcome.config.name) {
        return Err(DytesError::config(format!(
            "包名不能包含路径分隔符或 ..: {}",
            outcome.config.name
        )));
    }
    let work_dir = prepare_work_dir(&setup.dytes_dir, explicit_work_dir)?;
    let mut files = Vec::new();

    report.pass("演练目录已创建", work_dir.display().to_string());

    // 1. 清单落到包目录，模拟安装后可供插件库 / 更新引擎读取。
    let manifest_dir = match pkg_type {
        PackageType::App | PackageType::Core => work_dir.clone(),
        PackageType::Sys => work_dir.join("libary").join(&outcome.config.name),
        other => work_dir.join(export_dir(other, &outcome.config.name)),
    };
    fs::create_dir_all(&manifest_dir)
        .map_err(|e| DytesError::failed(format!("创建演练目录失败: {e}")))?;
    let manifest_path = manifest_dir.join("dytes.json");
    fs::write(&manifest_path, &outcome.manifest_text)
        .map_err(|e| DytesError::failed(format!("写入演练清单失败: {e}")))?;
    report.pass("清单已写入演练目录", manifest_path.display().to_string());

    // 2. 选择平台导出。`--manifest-only` 允许没有产物。
    let mut loader: Vec<Vec<String>> = Vec::new();
    let mut exports = Vec::new();
    if !manifest_only {
        match platform::select_exports(&outcome.config, &outcome.platform) {
            Some((selected, _)) => exports = selected.iter().collect(),
            None => report.fail(
                "当前平台导出",
                format!("platform-exports 缺少 {}", outcome.platform),
            ),
        }
    }

    let mut seen_dest = HashSet::new();
    for (source, dest) in exports {
        let dest_path = Path::new(dest);
        let label = format!("导出文件 {}", dest);
        if !is_safe_relative(dest_path) || dest_path.as_os_str().is_empty() {
            report.fail(label, "路径不安全（绝对路径或包含 ..）");
            continue;
        }
        if !seen_dest.insert(dest.clone()) {
            report.fail(label, "导出目标重复");
            continue;
        }

        let source_rel = Path::new(source);
        if !is_safe_relative(source_rel) || source_rel.as_os_str().is_empty() {
            report.fail(label, "源路径不安全（绝对路径或包含 ..）");
            continue;
        }
        let source_path = dir.join(source_rel);
        if !source_path.is_file() {
            report.fail(label, format!("源文件缺失: {}", source_path.display()));
            continue;
        }

        if let Some(expected) = checksum_for(outcome, dest) {
            match verify_file(&source_path, expected) {
                Ok(()) => report.pass(format!("checksum {dest}"), expected.sha256.clone()),
                Err(error) => {
                    report.fail(format!("checksum {dest}"), format!("{error:#}"));
                    continue;
                }
            }
        }

        let target = target_path(&work_dir, pkg_type, &outcome.config.name, dest_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| DytesError::failed(format!("创建导出目录失败: {e}")))?;
        }
        if let Err(error) = fs::copy(&source_path, &target) {
            report.fail(label, format!("复制失败: {error}"));
            continue;
        }
        loader.push(vec![source.clone(), dest.clone()]);
        files.push(json!({
            "source": source,
            "target": target,
        }));
        report.pass(
            format!("导出文件可落盘: {dest}"),
            target.display().to_string(),
        );
    }

    // 3. init 脚本必须存在且可复制；安装时会被放到包目录。
    let mut seen_init = HashSet::new();
    for script in &outcome.config.init {
        let path = Path::new(script);
        let label = format!("init 脚本 {script}");
        if !is_safe_relative(path) || path.as_os_str().is_empty() {
            report.fail(label, "路径不安全（绝对路径或包含 ..）");
            continue;
        }
        let Some(file_name) = path.file_name() else {
            report.fail(label, "缺少文件名");
            continue;
        };
        if !seen_init.insert(file_name.to_string_lossy().to_string()) {
            report.fail(label, "init 文件重复");
            continue;
        }
        let source_rel = Path::new(script);
        if !is_safe_relative(source_rel) {
            report.fail(label, "源路径不安全（绝对路径或包含 ..）");
            continue;
        }
        let source_path = dir.join(source_rel);
        if !source_path.is_file() {
            report.fail(label, format!("源文件缺失: {}", source_path.display()));
            continue;
        }
        let target = manifest_dir.join(file_name);
        if let Err(error) = fs::copy(&source_path, &target) {
            report.fail(label, format!("复制失败: {error}"));
            continue;
        }
        files.push(json!({
            "source": script,
            "target": target,
        }));
        report.pass(
            format!("init 脚本可落盘: {script}"),
            target.display().to_string(),
        );
    }

    // 4. 写入锁文件并立即回读，验证结构与序列化。
    let lock = DytesLock {
        package: vec![PackageLock {
            name: outcome.config.name.clone(),
            uuid: outcome.config.uuid.clone(),
            version: outcome.config.version.clone(),
            declared: Some(outcome.config.version.clone()),
            pkg_type: pkg_type.as_str().to_string(),
            loader,
        }],
    };
    let lock_path = work_dir.join("dytes.lock");
    let lock_text = serde_json::to_string_pretty(&lock)
        .map_err(|e| DytesError::failed(format!("锁文件序列化失败: {e}")))?;
    fs::write(&lock_path, &lock_text)
        .map_err(|e| DytesError::failed(format!("写入锁文件失败: {e}")))?;
    match serde_json::from_str::<DytesLock>(&lock_text) {
        Ok(_) => report.pass("锁文件可序列化", lock_path.display().to_string()),
        Err(error) => report.fail("锁文件可序列化", format!("{error}")),
    }

    if !keep {
        fs::remove_dir_all(&work_dir)
            .map_err(|e| DytesError::failed(format!("清理演练目录失败: {e}")))?;
    }

    Ok(Rehearsal {
        work_dir,
        kept: keep,
        files,
    })
}

/// 安装布局：导出文件相对于安装根目录的位置。
fn export_dir(pkg_type: PackageType, name: &str) -> PathBuf {
    match pkg_type {
        PackageType::App | PackageType::Core => PathBuf::new(),
        PackageType::Sys => PathBuf::from("libary"),
        PackageType::Web => Path::new("web").join(name),
        PackageType::DesignPlugins => Path::new("plugins").join(name),
        PackageType::DesignSoft => Path::new("apps").join(name),
        PackageType::Model => Path::new("models").join(name),
    }
}

/// 导出文件的落盘路径。
///
/// `sys` 清单习惯把目标写成 `libary/<file>`（相对安装根），也可能只写文件名；
/// 后者补上 `libary/`，前者不能重复添加。
fn target_path(work_dir: &Path, pkg_type: PackageType, name: &str, dest: &Path) -> PathBuf {
    match pkg_type {
        PackageType::Sys => {
            if dest.components().count() == 1 {
                work_dir.join("libary").join(dest)
            } else {
                work_dir.join(dest)
            }
        }
        _ => work_dir.join(export_dir(pkg_type, name)).join(dest),
    }
}

fn prepare_work_dir(dytes_dir: &Path, explicit: Option<PathBuf>) -> Result<PathBuf> {
    let work_dir = match explicit {
        Some(path) => {
            let relative = if path.is_absolute() {
                path.strip_prefix(dytes_dir)
                    .map_err(|_| {
                        DytesError::args(format!("--work-dir 必须位于 {} 内", dytes_dir.display()))
                    })?
                    .to_path_buf()
            } else {
                path
            };
            if !is_safe_relative(&relative) || relative.as_os_str().is_empty() {
                return Err(DytesError::args(
                    "--work-dir 必须是 .dytes 内的相对路径，且不能包含 ..",
                ));
            }
            dytes_dir.join(relative)
        }
        None => {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            dytes_dir
                .join(format!("run-{}", std::process::id()))
                .join(format!("dytes-test-{stamp}"))
        }
    };
    if work_dir.exists() {
        return Err(DytesError::args(format!(
            "演练目录已存在: {}；请换一个目录或先删除",
            work_dir.display()
        )));
    }
    fs::create_dir_all(&work_dir)
        .map_err(|e| DytesError::failed(format!("创建演练目录失败: {e}")))?;
    Ok(work_dir)
}

/// 包名会被拼进 `web/<name>` / `plugins/<name>` 等安装路径，必须只是单段名称。
fn is_safe_install_name(name: &str) -> bool {
    let path = Path::new(name);
    !name.is_empty()
        && path.components().count() == 1
        && is_safe_relative(path)
        && name != "."
        && name != ".."
}

/// 拒绝绝对路径、盘符与 `..`，保证演练不会写到隔离目录之外。
fn is_safe_relative(path: &Path) -> bool {
    !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

/// 校验和同时兼容 `dest` 与 `<platform>/dest` 两种键。
fn checksum_for<'a>(
    outcome: &'a CheckOutcome,
    key: &str,
) -> Option<&'a crate::utils::checksum::FileChecksum> {
    outcome
        .config
        .checksums
        .get(&format!("{}/{}", outcome.platform, key))
        .or_else(|| outcome.config.checksums.get(key))
}
