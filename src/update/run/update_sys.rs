use crate::update::utils::dytes::DytesConfig;
use crate::update::utils::env::get_app_dir;
use crate::update::utils::loader::{download_file, fetch_text};
use crate::update::utils::lock::{get_package_version_from_lock, update_package_in_lock};
use crate::update::utils::version::check_version;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// sys 包更新结果：包本地目录与需要执行的 init 脚本
pub struct SysUpdateResult {
    pub package_name: String,
    pub package_dir: String,
    pub init_scripts: Vec<String>,
}

/// 获取当前操作系统对应的库文件扩展名
fn get_lib_extension() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "dll"
    }
    #[cfg(target_os = "macos")]
    {
        "dylib"
    }
    #[cfg(target_os = "linux")]
    {
        "so"
    }
}

/// 根据操作系统匹配库文件名；非库文件（setup.ts / dytes.json 等）保持原名。
fn match_lib_filename(dest_filename: &str) -> String {
    let ext = get_lib_extension();
    let path = Path::new(dest_filename);

    let Some(file_ext) = path.extension().and_then(|value| value.to_str()) else {
        return format!("{dest_filename}.{ext}");
    };

    if !matches!(
        file_ext.to_ascii_lowercase().as_str(),
        "dll" | "so" | "dylib"
    ) {
        return dest_filename.to_string();
    }

    match path.file_stem().and_then(|value| value.to_str()) {
        Some(base_name) => format!("{base_name}.{ext}"),
        None => format!("{dest_filename}.{ext}"),
    }
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

/// 包本地目录：libary/<name>，用于保存 dytes.json 与 init 脚本
fn package_dir(name: &str) -> String {
    let app_dir = get_app_dir();
    format!("{}/libary/{}", app_dir.trim_end_matches('/'), name)
}

/// 把云端 dytes.json 保存到本地包目录
fn save_manifest(name: &str, text: &str) -> Result<String> {
    let dir = package_dir(name);
    fs::create_dir_all(&dir)?;
    fs::write(format!("{}/dytes.json", dir), text)?;
    println!("  已保存包清单: {}/dytes.json", dir);
    Ok(dir)
}

fn init_file_name(script: &str) -> Option<String> {
    Path::new(script.trim_start_matches("./"))
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
}

pub async fn update_sys_info(
    name: String,
    declared_version: String,
) -> Result<Option<SysUpdateResult>> {
    println!("更新系统库: {} (声明版本 {})", name, declared_version);

    // 1. 获取远程包信息（使用 S3 路径）
    let s3_path = format!("system/{}/dytes.json", name);
    println!("  从 S3 获取: {}", s3_path);

    let remote_text = fetch_text(&s3_path)
        .await
        .with_context(|| format!("获取 system/{}/dytes.json 失败", name))?;
    let remote_app_info: DytesConfig = serde_json::from_str(&remote_text)
        .with_context(|| format!("解析 system/{}/dytes.json 失败", name))?;

    let platform = crate::update::utils::platform::current_platform();
    let Some((exports, platform_tag)) =
        crate::update::utils::platform::select_exports(&remote_app_info, &platform)
    else {
        anyhow::bail!("包 {} 未发布平台 {}", name, platform);
    };
    let base_path = crate::update::utils::platform::remote_base(
        &format!("system/{}", name),
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
                save_manifest(&name, &remote_text)?;
                return Ok(None);
            }
        }
    }

    // 2. 获取应用目录 - libary/
    let app_dir = get_app_dir();
    let lib_dir = format!("{}/libary", app_dir.trim_end_matches('/'));
    fs::create_dir_all(&lib_dir)?;

    // 3. 下载导出的文件（使用 S3 路径）
    for dest_filename in exports.values() {
        // 平台专用包直接使用清单中的文件名；旧布局按当前系统替换扩展名
        let remote_filename = if platform_tag.is_some() {
            dest_filename.clone()
        } else {
            match_lib_filename(dest_filename)
        };

        // 构建 S3 路径
        let s3_path = format!("{}{}", base_path, remote_filename);

        // 本地目标路径 - 使用原始 dest_filename（保持配置文件中的名称）
        let dest_path = format!("{}/{}", lib_dir, dest_filename);

        println!("  下载: {} -> {}", s3_path, dest_path);

        // 下载文件到 lib_dir
        if !download_file(&s3_path, &lib_dir).await {
            anyhow::bail!("下载系统库 {} 失败", remote_filename);
        }

        // 如果下载的文件名与目标文件名不同，需要重命名
        let downloaded_path = format!("{}/{}", lib_dir, remote_filename);
        if downloaded_path != dest_path && std::path::Path::new(&downloaded_path).exists() {
            if std::path::Path::new(&dest_path).exists() {
                std::fs::remove_file(&dest_path)?;
            }
            std::fs::rename(&downloaded_path, &dest_path)?;
            println!("  重命名: {} -> {}", remote_filename, dest_filename);
        }

        println!("  下载完成: {}", dest_filename);
    }

    // 4. 保存包清单，并把 init 脚本移动到本地包目录，避免多个 sys 包互相覆盖
    let package_dir = save_manifest(&name, &remote_text)?;
    let init_files: HashSet<String> = remote_app_info
        .init
        .iter()
        .filter_map(|script| init_file_name(script))
        .collect();

    let mut init_scripts = Vec::new();
    for file_name in &init_files {
        let downloaded_path = format!("{}/{}", lib_dir, file_name);
        let target_path = format!("{}/{}", package_dir, file_name);

        if std::path::Path::new(&downloaded_path).exists() {
            if std::path::Path::new(&target_path).exists() {
                fs::remove_file(&target_path)?;
            }
            fs::rename(&downloaded_path, &target_path)?;
            println!("  init 脚本: {} -> {}", downloaded_path, target_path);
            init_scripts.push(format!("./{file_name}"));
        } else if std::path::Path::new(&target_path).exists() {
            init_scripts.push(format!("./{file_name}"));
        } else {
            println!("  警告: init 脚本 {} 未下载，跳过", file_name);
        }
    }

    // 5. 构建 loader 数据；init 脚本记录实际位置（包目录）
    let mut loader = Vec::new();
    for (source, dest) in exports {
        let file_name = Path::new(dest)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();

        if init_files.contains(&file_name) {
            loader.push(vec![source.clone(), format!("{}/{}", name, file_name)]);
        } else {
            loader.push(vec![source.clone(), dest.clone()]);
        }
    }

    // 6. 写入锁文件 - 使用 utils::lock
    update_package_in_lock(
        remote_app_info.uuid.as_deref(),
        &name,
        &remote_app_info.version,
        "sys",
        loader,
    )?;

    println!("  系统库更新完成: {} v{}", name, remote_app_info.version);

    if init_scripts.is_empty() {
        Ok(None)
    } else {
        Ok(Some(SysUpdateResult {
            package_name: name,
            package_dir,
            init_scripts,
        }))
    }
}
