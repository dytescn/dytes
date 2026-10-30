use anyhow::{Context, Result};
use std::fs;

use crate::update::utils::dytes::DytesConfig;
use crate::update::utils::env::get_app_dir;
use crate::update::utils::loader::{download_file, fetch_text};
use crate::update::utils::lock::{get_package_version_from_lock, update_package_in_lock};
use crate::update::utils::version::check_version;

/// 设计包类型：插件或设计软件
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DesignKind {
    Plugin,
    Soft,
}

impl DesignKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DesignKind::Plugin => "design_plugins",
            DesignKind::Soft => "design_soft",
        }
    }

    fn remote_dir(self) -> &'static str {
        match self {
            DesignKind::Plugin => "design_plugins",
            DesignKind::Soft => "design_soft",
        }
    }

    fn local_dir(self) -> &'static str {
        match self {
            DesignKind::Plugin => "plugins",
            DesignKind::Soft => "apps",
        }
    }

    fn label(self) -> &'static str {
        match self {
            DesignKind::Plugin => "插件",
            DesignKind::Soft => "软件",
        }
    }
}

/// 单个设计包的安装 / 更新结果。
#[derive(Debug, Clone)]
pub struct DesignUpdateResult {
    pub name: String,
    /// 包清单 uuid；旧包可能为空。
    pub uuid: Option<String>,
    pub version: String,
    /// 安装目录（相对安装根目录）。
    pub path: String,
    /// 本次安装的导出文件。
    pub files: Vec<String>,
    /// 本地版本已不低于线上版本，未执行下载。
    pub skipped: bool,
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

pub async fn update_design_info(
    name: String,
    declared_version: String,
    kind: DesignKind,
    force: bool,
    expected_version: Option<&str>,
) -> Result<DesignUpdateResult> {
    let remote_dir = kind.remote_dir();
    println!(
        "更新设计{}: {} (声明版本 {})",
        kind.label(),
        name,
        declared_version
    );

    // 1. 获取远程包信息
    let s3_path = format!("{}/{}/dytes.json", remote_dir, name);
    println!("  从 S3 获取: {}", s3_path);

    let remote_text = fetch_text(&s3_path)
        .await
        .with_context(|| format!("获取 {} 失败", s3_path))?;
    let remote_info: DytesConfig =
        serde_json::from_str(&remote_text).with_context(|| format!("解析 {} 失败", s3_path))?;

    if let Some(expected) = expected_version {
        if expected != remote_info.version {
            anyhow::bail!(
                "指定版本 {} 与线上最新版本 {} 不一致；当前只支持安装最新版本",
                expected,
                remote_info.version
            );
        }
    }

    let platform = crate::update::utils::platform::current_platform();
    let Some((exports, platform_tag)) =
        crate::update::utils::platform::select_exports(&remote_info, &platform)
    else {
        anyhow::bail!("包 {} 未发布平台 {}", name, platform);
    };
    let base_path = crate::update::utils::platform::remote_base(
        &format!("{}/{}", remote_dir, name),
        &remote_info.version,
        platform_tag,
    );

    println!("  远程版本: {}", remote_info.version);
    println!("  平台: {}", platform);
    println!("  导出文件: {:?}", exports);

    // 2. 安装目录：插件 -> plugins/<name>；设计软件 -> apps/<name>
    let app_dir = get_app_dir();
    let package_dir = format!(
        "{}/{}/{}",
        app_dir.trim_end_matches('/'),
        kind.local_dir(),
        name
    );
    let files: Vec<String> = exports.values().cloned().collect();

    // 3. 版本比较：锁文件版本不低于远程版本时跳过
    if !force {
        if let Ok(Some(lock_version)) = get_package_version_from_lock(&name) {
            if let (Some(local), Some(remote)) = (
                parse_version(&lock_version),
                parse_version(&remote_info.version),
            ) {
                if !check_version(&local, &remote) {
                    println!("  已是最新版本 (v{})，跳过下载", local);
                    return Ok(DesignUpdateResult {
                        name,
                        uuid: remote_info.uuid.clone(),
                        version: remote_info.version.clone(),
                        path: package_dir,
                        files,
                        skipped: true,
                    });
                }
            }
        }
    }

    fs::create_dir_all(&package_dir)?;
    println!("  安装目录: {}", package_dir);

    // 4. 下载导出文件
    for dest_filename in exports.values() {
        if dest_filename.contains('/') || dest_filename.contains('\\') {
            anyhow::bail!("不支持的导出路径（只允许文件名）: {}", dest_filename);
        }

        let s3_file = format!("{}{}", base_path, dest_filename);
        println!("  下载: {} -> {}/{}", s3_file, package_dir, dest_filename);

        if !download_file(&s3_file, &package_dir).await {
            anyhow::bail!("下载 {}/{} 失败", name, dest_filename);
        }
    }

    // 5. 保存云端清单，供插件管理库读取 use_ai / targets，也供设计软件读取入口
    let manifest_path = format!("{package_dir}/dytes.json");
    fs::write(&manifest_path, &remote_text)
        .with_context(|| format!("保存 {} 失败", manifest_path))?;
    println!("  保存清单: {}", manifest_path);

    // 6. 设计插件额外注册到 libary，供 cyctron 加载
    if kind == DesignKind::Plugin {
        let lib_dir = format!("{}/libary", app_dir.trim_end_matches('/'));
        fs::create_dir_all(&lib_dir)?;
        for dest_filename in exports.values() {
            let lower = dest_filename.to_ascii_lowercase();
            if lower.ends_with(".dll") || lower.ends_with(".so") || lower.ends_with(".dylib") {
                let src = format!("{}/{}", package_dir, dest_filename);
                let dst = format!("{}/{}", lib_dir, dest_filename);
                if std::path::Path::new(&src).exists() {
                    fs::copy(&src, &dst)?;
                    println!("  插件注册到 libary: {}", dst);
                }
            }
        }
    }

    // 7. 写锁文件
    let mut loader = Vec::new();
    for (source, dest) in exports {
        loader.push(vec![source.clone(), dest.clone()]);
    }
    update_package_in_lock(
        remote_info.uuid.as_deref(),
        &name,
        &remote_info.version,
        remote_dir,
        loader,
    )?;

    println!(
        "  设计{}更新完成: {} v{}",
        kind.label(),
        name,
        remote_info.version
    );

    Ok(DesignUpdateResult {
        name,
        uuid: remote_info.uuid.clone(),
        version: remote_info.version,
        path: package_dir,
        files,
        skipped: false,
    })
}
