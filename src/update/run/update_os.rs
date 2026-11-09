use crate::shared::checksum::verify_download;
use crate::update::utils::dytes::DytesConfig;
use crate::update::utils::env::get_app_dir;
use crate::update::utils::executable;
use crate::update::utils::loader::{download_file, fetch_text};
use crate::update::utils::lock::{get_package_version_from_lock, update_package_in_lock};
use crate::update::utils::version::check_version;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Unix 下给可执行产物设置执行位。
///
/// S3 / 下载落盘不会保留源文件的权限，`cyctron`、`update` 等无扩展名的
/// 可执行文件默认是 0644，启动时会 `Permission denied`。
#[cfg(unix)]
fn ensure_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

/// 只对非文本导出设置执行位，避免把 `setup.ts` / `mcp.md` 误标为可执行。
#[cfg(unix)]
fn ensure_executable_if_binary(path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        ext.as_str(),
        "json" | "md" | "ts" | "txt" | "toml" | "yml" | "yaml" | "dll" | "so" | "dylib"
    ) {
        return Ok(());
    }
    ensure_executable(path)
}

#[cfg(not(unix))]
fn ensure_executable_if_binary(_path: &Path) -> Result<()> {
    Ok(())
}

/// 去除版本号前缀（^ / ~）
fn parse_version(version: &str) -> Option<String> {
    let cleaned = version.trim_start_matches('^').trim_start_matches('~');
    if cleaned.parse::<semver::Version>().is_ok() {
        Some(cleaned.to_string())
    } else {
        None
    }
}

pub async fn update_os_info(name: String, declared_version: String) -> Result<()> {
    println!("更新核心包: {} (声明版本 {})", name, declared_version);

    // 1. 获取远程包信息（使用 S3 路径）
    let s3_path = format!("core/{}/dytes.json", name);
    println!("  从 S3 获取: {}", s3_path);

    let remote_text = fetch_text(&s3_path)
        .await
        .with_context(|| format!("获取 {}/dytes.json 失败", name))?;
    let remote_app_info: DytesConfig = serde_json::from_str(&remote_text)
        .with_context(|| format!("解析 {}/dytes.json 失败", name))?;

    let platform = crate::update::utils::platform::current_platform();
    let Some((exports, platform_tag)) =
        crate::update::utils::platform::select_exports(&remote_app_info, &platform)
    else {
        anyhow::bail!("包 {} 未发布平台 {}", name, platform);
    };
    let base_path = crate::update::utils::platform::remote_base(
        &format!("core/{}", name),
        &remote_app_info.version,
        platform_tag,
    );

    println!("  远程版本: {}", remote_app_info.version);
    println!("  平台: {}", platform);
    println!("  导出文件: {:?}", exports);

    // 声明版本可能滞后于远程实际版本，以远程版本为准判断是否真的需要更新
    if let Ok(Some(lock_version)) = get_package_version_from_lock(&name) {
        if let (Some(local), Some(remote)) = (
            parse_version(&lock_version),
            parse_version(&remote_app_info.version),
        ) {
            if !check_version(&local, &remote) {
                println!("  已是最新版本 (v{})，跳过下载", local);
                // 兼容旧安装：下载时未设置执行位的 core 产物在此补一次。
                let app_dir = get_app_dir();
                for dest_filename in exports.values() {
                    let path = format!("{}/{}", app_dir.trim_end_matches('/'), dest_filename);
                    let _ = ensure_executable_if_binary(Path::new(&path));
                }
                return Ok(());
            }
        }
    }

    // 运行中的 update.exe 无法覆盖自身：下载为 update.new.exe，由 Piksel 启动时替换
    if name == "update" {
        let app_dir = get_app_dir();
        let tmp_dir = format!("{}/.update_tmp", app_dir.trim_end_matches('/'));
        fs::create_dir_all(&tmp_dir)?;

        let mut downloaded_file: Option<String> = None;
        for dest_filename in exports.values() {
            let s3_path = format!("{}{}", base_path, dest_filename);
            println!("  下载: {} -> {}/{}", s3_path, tmp_dir, dest_filename);
            if !download_file(&s3_path, &tmp_dir).await {
                anyhow::bail!("下载 {} 失败", dest_filename);
            }
            verify_download(
                std::path::Path::new(&format!("{}/{}", tmp_dir, dest_filename)),
                dest_filename,
                &remote_app_info.checksums,
            )?;
            downloaded_file = Some(format!("{}/{}", tmp_dir, dest_filename));
        }

        if let Some(src) = downloaded_file {
            let dst = format!("{}/update.new.exe", app_dir.trim_end_matches('/'));
            let _ = fs::remove_file(&dst);
            fs::rename(&src, &dst)?;
            ensure_executable_if_binary(Path::new(&dst))?;
            println!("  已准备新版本: {}（Piksel 启动时替换 update.exe）", dst);

            let mut loader = Vec::new();
            for (source, dest) in exports {
                loader.push(vec![source.clone(), dest.clone()]);
            }
            update_package_in_lock(
                remote_app_info.uuid.as_deref(),
                &name,
                &remote_app_info.version,
                Some(&declared_version),
                "core",
                loader,
            )?;
            let _ = fs::remove_dir_all(&tmp_dir);
            println!("  更新包已准备: update v{}", remote_app_info.version);
        }
        return Ok(());
    }

    // 2. 获取应用目录 - 直接放到 ./
    let app_dir = get_app_dir();
    fs::create_dir_all(&app_dir)?;

    // 3. 下载导出的文件（使用 S3 路径）
    for dest_filename in exports.values() {
        let s3_path = format!("{}{}", base_path, dest_filename);

        // 下载到 ./{文件名}，确保路径正确
        let dest_path = format!("{}/{}", app_dir.trim_end_matches('/'), dest_filename);

        println!("  下载: {} -> {}", s3_path, dest_path);

        // 下载文件到 app_dir
        if !download_file(&s3_path, &app_dir).await {
            anyhow::bail!("下载 {} 失败", dest_filename);
        }

        verify_download(
            std::path::Path::new(&dest_path),
            dest_filename,
            &remote_app_info.checksums,
        )?;
        // S3 不保留执行位：cyctron / dytes / update 等 core 产物需要 0755。
        ensure_executable_if_binary(Path::new(&dest_path))?;
        // Linux / macOS 的 .bin / .app 产物额外创建无后缀命令入口。
        executable::install_command_alias(Path::new(&app_dir), dest_filename)?;
        println!("  下载完成: {}", dest_filename);
    }

    // 4. 构建 loader 数据
    let mut loader = Vec::new();
    for (source, dest) in exports {
        loader.push(vec![source.clone(), dest.clone()]);
    }

    // 5. 写入锁文件 - 使用 utils::lock
    update_package_in_lock(
        remote_app_info.uuid.as_deref(),
        &name,
        &remote_app_info.version,
        Some(&declared_version),
        "core",
        loader,
    )?;

    println!("  更新完成: {} v{}", name, remote_app_info.version);
    Ok(())
}
