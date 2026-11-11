//! `dytes self-update`：检查并安装 CLI 自身的新版本。
//!
//! 与 `dytes update` 不同，本模块只处理 dytes 二进制本身，不读取当前目录的
//! `dytes.json`，也不更新应用依赖。
//!
//! 版本清单、二进制与 `.sha256` 全部直接走 S3（对象存储），不读取
//! `version.js`，也不经过 packages.funxdata.com 的 CDN：CDN 缓存会让更新读到
//! 旧版本或下载到不存在的文件。下载完成后校验 SHA-256，再原子替换。

use crate::update::loader;
use crate::utils::dytes::DytesConfig;
use crate::utils::platform;
use anyhow::{bail, Context, Result};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const S3_DYTES_PREFIX: &str = "core/dytes";

async fn fetch_s3_manifest() -> Result<DytesConfig> {
    let key = format!("{S3_DYTES_PREFIX}/dytes.json");
    let text = loader::fetch_text(&key).await?;
    serde_json::from_str(&text).with_context(|| format!("解析 S3 清单失败: {key}"))
}

/// 读取指定版本的清单；发布流程会同时上传 `core/dytes/<version>/dytes.json`。
async fn fetch_s3_versioned_manifest(version: &str) -> Option<DytesConfig> {
    let key = format!("{S3_DYTES_PREFIX}/{version}/dytes.json");
    let text = loader::fetch_text(&key).await.ok()?;
    serde_json::from_str(&text).ok()
}

fn s3_binary_key(version: &str, platform: &str, binary: &str) -> String {
    format!("{S3_DYTES_PREFIX}/{version}/{platform}/{binary}")
}

fn s3_checksum_key(version: &str, platform: &str, binary: &str) -> String {
    format!("{}.sha256", s3_binary_key(version, platform, binary))
}

async fn download_s3_binary(
    version: &str,
    platform: &str,
    binary: &str,
    temp: &Path,
) -> Result<String> {
    let key = s3_binary_key(version, platform, binary);
    let dir = temp.with_file_name(format!(".dytes-s3-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).with_context(|| format!("创建临时目录失败: {}", dir.display()))?;

    let dir_str = dir.to_str().context("临时目录路径不是有效 UTF-8")?;
    let downloaded = loader::download_file(&key, dir_str).await;
    if !downloaded {
        let _ = fs::remove_dir_all(&dir);
        bail!("S3 下载失败: {key}（请检查 S3 凭证 / 网络）");
    }

    let downloaded_path = dir.join(binary);
    if !downloaded_path.exists() {
        let _ = fs::remove_dir_all(&dir);
        bail!("S3 下载完成但未找到文件: {}", downloaded_path.display());
    }

    let _ = fs::remove_file(temp);
    fs::rename(&downloaded_path, temp)
        .with_context(|| format!("移动 S3 下载文件失败: {}", temp.display()))?;
    let _ = fs::remove_dir_all(&dir);
    Ok(key)
}

async fn verify_s3_checksum(
    version: &str,
    platform: &str,
    binary: &str,
    temp: &Path,
) -> Result<()> {
    let key = s3_checksum_key(version, platform, binary);
    match loader::fetch_text(&key).await {
        Ok(text) => match text.split_whitespace().next() {
            Some(expected) => verify_sha256(temp, expected),
            None => {
                println!("  警告: S3 校验文件为空: {key}，跳过校验");
                Ok(())
            }
        },
        Err(error) => {
            println!("  警告: 未找到 S3 校验文件 {key}（{error:#}），跳过校验");
            Ok(())
        }
    }
}

fn sha256_file(file: &Path) -> Result<String> {
    let mut file =
        fs::File::open(file).with_context(|| format!("打开文件失败: {}", file.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).context("读取文件失败")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn verify_sha256(file: &Path, expected: &str) -> Result<()> {
    let actual = sha256_file(file)?;
    if !actual.eq_ignore_ascii_case(expected.trim()) {
        bail!("SHA-256 校验失败: 期望 {expected}，实际 {actual}");
    }
    println!("  SHA-256 校验通过");
    Ok(())
}

pub struct SelfUpdateResult {
    pub current: String,
    pub latest: String,
    pub updated: bool,
    pub path: PathBuf,
}

fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn binary_name(config: &DytesConfig, platform: &str) -> Result<String> {
    let Some((exports, _)) = platform::select_exports(config, platform) else {
        bail!("dytes 未发布平台 {platform}");
    };
    exports
        .values()
        .find(|name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with(".exe") || !lower.contains('.')
        })
        .cloned()
        .or_else(|| exports.values().next().cloned())
        .context("dytes 清单没有导出文件")
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn replace_binary(temp: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let backup = target.with_extension("exe.old");
        let _ = fs::remove_file(&backup);
        match fs::rename(target, &backup) {
            Ok(()) => match fs::rename(temp, target) {
                Ok(()) => {
                    let _ = fs::remove_file(&backup);
                }
                Err(error) => {
                    let _ = fs::rename(&backup, target);
                    bail!("替换 dytes 失败: {error}");
                }
            },
            Err(error) => {
                // 运行中的 exe 被锁定时，交给下一个进程退出后再替换。
                let script = format!(
                    "ping 127.0.0.1 -n 3 >nul & move /Y \"{}\" \"{}\"",
                    temp.display(),
                    target.display()
                );
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                std::process::Command::new("cmd")
                    .args(["/C", &script])
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()
                    .with_context(|| format!("启动替换进程失败: {error}"))?;
                println!("  当前进程退出后将自动替换: {}", target.display());
            }
        }
    }

    #[cfg(not(windows))]
    {
        fs::rename(temp, target).with_context(|| format!("替换 {} 失败", target.display()))?;
    }

    Ok(())
}

pub async fn run(check: bool, force: bool, json_output: bool) -> Result<SelfUpdateResult> {
    let current = current_version().to_string();
    let platform = platform::current_platform();
    let target = std::env::current_exe().context("获取当前 dytes 路径失败")?;

    let manifest = fetch_s3_manifest()
        .await
        .context("读取 S3 dytes 清单失败；请检查 S3 凭证或网络")?;
    let latest = manifest.version.clone();
    println!("  S3 版本: v{latest}");

    // 版本化清单与未版本化清单同版本；读取失败时回退未版本化清单。
    let config = match fetch_s3_versioned_manifest(&latest).await {
        Some(config) => config,
        None => manifest,
    };

    let current_semver = semver::Version::parse(&current).context("当前版本不是合法 semver")?;
    let latest_semver = semver::Version::parse(&latest).context("线上版本不是合法 semver")?;
    let has_update = latest_semver > current_semver;

    if check || (!has_update && !force) {
        let value = json!({
            "current": current,
            "latest": latest,
            "manifest_version": latest,
            "version_source": "s3",
            "platform": platform,
            "has_update": has_update,
            "path": target,
        });
        if json_output {
            println!("{value}");
        } else if has_update {
            println!("dytes 有新版本: v{current} -> v{latest}（{platform}）");
            println!("运行 dytes self-update 升级");
        } else {
            println!("dytes 已是最新版本: v{current}");
        }
        return Ok(SelfUpdateResult {
            current,
            latest,
            updated: false,
            path: target,
        });
    }

    let temp = target.with_file_name(format!(
        ".{}.download.{}",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dytes"),
        std::process::id()
    ));

    println!("更新 dytes: v{current} -> v{latest}（{platform}）");

    let binary = binary_name(&config, &platform)?;
    let key = download_s3_binary(&latest, &platform, &binary, &temp).await?;
    verify_s3_checksum(&latest, &platform, &binary, &temp).await?;

    set_executable(&temp)?;
    replace_binary(&temp, &target)?;

    let value = json!({
        "current": current,
        "latest": latest,
        "platform": platform,
        "updated": true,
        "source": "s3",
        "download_path": key,
        "path": target,
    });
    if json_output {
        println!("{value}");
    } else {
        println!("dytes 已更新到 v{latest}（s3）: {}", target.display());
    }
    Ok(SelfUpdateResult {
        current,
        latest,
        updated: true,
        path: target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> DytesConfig {
        serde_json::from_str(
            r#"{
                "name": "dytes",
                "version": "0.1.11",
                "type": "core",
                "platform-exports": {
                    "windows": {"./target/release/dytes.exe": "dytes.exe"},
                    "linux": {"./target/release/dytes": "dytes.bin"},
                    "macos": {"./target/release/dytes": "dytes.app"}
                }
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn picks_platform_binary() {
        let config = manifest();
        assert_eq!(binary_name(&config, "windows").unwrap(), "dytes.exe");
        assert_eq!(binary_name(&config, "linux").unwrap(), "dytes.bin");
        assert_eq!(binary_name(&config, "macos").unwrap(), "dytes.app");
    }

    #[test]
    fn builds_s3_object_keys() {
        assert_eq!(
            s3_binary_key("0.1.11", "windows", "dytes.exe"),
            "core/dytes/0.1.11/windows/dytes.exe"
        );
        assert_eq!(
            s3_checksum_key("0.1.11", "linux", "dytes.bin"),
            "core/dytes/0.1.11/linux/dytes.bin.sha256"
        );
    }
}
