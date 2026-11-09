//! `dytes self-update`：检查并安装 CLI 自身的新版本。
//!
//! 与 `dytes update` 不同，本模块只处理 dytes 二进制本身，不读取当前目录的
//! `dytes.json`，也不更新应用依赖。
//!
//! 下载来源默认 `auto`：优先 S3（无缓存，能拿到最新版本），S3 不可用时回退 CDN；
//! `--source cdn|s3` 可强制单一来源。S3 / CDN 下载均校验同名 `.sha256`，最后原子替换。

use crate::shared::config::DytesConfig;
use crate::shared::platform;
use crate::update::utils::loader;
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_CDN_BASE: &str = "https://packages.funxdata.com/core/dytes";
const RETRIES: u32 = 3;

/// CDN 根地址；可用 `DYTES_DOWNLOAD_BASE` 覆盖（与安装脚本一致）。
fn cdn_base() -> String {
    std::env::var("DYTES_DOWNLOAD_BASE")
        .ok()
        .map(|value| value.trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_CDN_BASE.to_string())
}

fn manifest_url() -> String {
    format!("{}/dytes.json", cdn_base())
}

fn version_script_url() -> String {
    format!("{}/latest/version.js", cdn_base())
}

const S3_DYTES_PREFIX: &str = "core/dytes";

async fn fetch_s3_manifest() -> Result<DytesConfig> {
    let key = format!("{S3_DYTES_PREFIX}/dytes.json");
    let text = loader::fetch_text(&key).await?;
    serde_json::from_str(&text).with_context(|| format!("解析 S3 清单失败: {key}"))
}

async fn fetch_s3_release_version() -> Option<String> {
    let key = format!("{S3_DYTES_PREFIX}/latest/version.js");
    let text = loader::fetch_text(&key).await.ok()?;
    parse_release_version_script(&text)
}

fn s3_binary_key(version: &str, platform: &str, binary: &str) -> String {
    format!("{S3_DYTES_PREFIX}/{version}/{platform}/{binary}")
}

fn s3_checksum_key(version: &str, platform: &str, binary: &str) -> String {
    format!("{}.sha256", s3_binary_key(version, platform, binary))
}

async fn verify_checksum(client: &reqwest::Client, url: &str, file: &Path) -> Result<()> {
    let checksum_url = format!("{url}.sha256");
    let expected = match client.get(&checksum_url).send().await {
        Ok(response) if response.status().is_success() => {
            let text = response.text().await.context("读取校验文件失败")?;
            text.split_whitespace().next().map(str::to_string)
        }
        _ => None,
    };
    let Some(expected) = expected else {
        println!("  警告: 未找到 {checksum_url}，跳过校验");
        return Ok(());
    };
    verify_sha256(file, &expected)
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

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(15))
        .user_agent(format!("dytes/{}", current_version()))
        .build()
        .context("构建 HTTP 客户端失败")
}

async fn fetch_manifest(client: &reqwest::Client) -> Result<DytesConfig> {
    let url = manifest_url();
    let mut last_error = String::new();
    for attempt in 1..=RETRIES {
        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                let text = response.text().await.context("读取 dytes 清单失败")?;
                return serde_json::from_str(&text).context("解析 dytes 清单失败");
            }
            Ok(response) => last_error = format!("HTTP {}", response.status()),
            Err(error) => last_error = error.to_string(),
        }
        if attempt < RETRIES {
            println!("  读取清单失败 (尝试 {attempt}/{RETRIES}): {last_error}，稍后重试...");
            tokio::time::sleep(Duration::from_secs(attempt as u64)).await;
        }
    }
    bail!("读取 {url} 失败: {last_error}")
}

/// 读取指定版本的 CDN 清单（不可变路径，不受未版本化清单缓存影响）。
async fn fetch_versioned_manifest(client: &reqwest::Client, version: &str) -> Result<DytesConfig> {
    let url = format!("{}/{}/dytes.json", cdn_base(), version);
    let mut last_error = String::new();
    for attempt in 1..=RETRIES {
        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                let text = response.text().await.context("读取版本化 dytes 清单失败")?;
                return serde_json::from_str(&text).context("解析版本化 dytes 清单失败");
            }
            Ok(response) => last_error = format!("HTTP {}", response.status()),
            Err(error) => last_error = error.to_string(),
        }
        if attempt < RETRIES {
            println!("  读取版本化清单失败 (尝试 {attempt}/{RETRIES}): {last_error}，稍后重试...");
            tokio::time::sleep(Duration::from_secs(attempt as u64)).await;
        }
    }
    bail!("读取 {url} 失败: {last_error}")
}

/// 读取 `latest/version.js` 中的发布版本。
///
/// 清单可能被 CDN 缓存成旧版本，而 `version.js` 与 `latest` 别名同源；
/// 用它作为“实际发布版本”可以避免 `--force` 下载新版却显示旧版本号。
async fn fetch_release_version(client: &reqwest::Client) -> Option<String> {
    let response = client.get(version_script_url()).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let text = response.text().await.ok()?;
    parse_release_version_script(&text)
}

fn parse_release_version_script(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let value: serde_json::Value = serde_json::from_str(&text[start..=end]).ok()?;
    value
        .get("version")
        .and_then(|version| version.as_str())
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(str::to_string)
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

fn download_url(version: &str, platform: &str, binary: &str) -> String {
    format!("{}/{}/{}/{}", cdn_base(), version, platform, binary)
}

async fn download_file(client: &reqwest::Client, url: &str, dest: &Path) -> Result<()> {
    download_file_with_options(client, url, dest, None, None).await
}

async fn download_file_with_options(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    accept: Option<&str>,
    token: Option<&str>,
) -> Result<()> {
    let mut last_error = String::new();
    for attempt in 1..=RETRIES {
        let mut request = client.get(url);
        if let Some(accept) = accept {
            request = request.header("Accept", accept);
        }
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        match request.send().await {
            Ok(response) if response.status().is_success() => {
                let mut file = tokio::fs::File::create(dest)
                    .await
                    .with_context(|| format!("创建临时文件失败: {}", dest.display()))?;
                let mut stream = response.bytes_stream();
                let mut write_error = None;
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(bytes) => {
                            if let Err(error) =
                                tokio::io::AsyncWriteExt::write_all(&mut file, &bytes).await
                            {
                                write_error = Some(error.to_string());
                                break;
                            }
                        }
                        Err(error) => {
                            write_error = Some(error.to_string());
                            break;
                        }
                    }
                }
                if let Some(error) = write_error {
                    last_error = error;
                } else {
                    tokio::io::AsyncWriteExt::flush(&mut file)
                        .await
                        .context("写入临时文件失败")?;
                    return Ok(());
                }
            }
            Ok(response) => last_error = format!("HTTP {}", response.status()),
            Err(error) => last_error = error.to_string(),
        }
        let _ = fs::remove_file(dest);
        if attempt < RETRIES {
            println!("  下载失败 (尝试 {attempt}/{RETRIES}): {last_error}，稍后重试...");
            tokio::time::sleep(Duration::from_secs(attempt as u64)).await;
        }
    }
    bail!("下载 {url} 失败: {last_error}")
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

/// self-update 的下载来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateSource {
    /// 优先 S3（无缓存），CDN 作为回退。
    Auto,
    /// 只走 packages.funxdata.com（CDN）。
    Cdn,
    /// 只走 S3 API。
    S3,
}

impl UpdateSource {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "cdn" | "packages" => Some(Self::Cdn),
            "s3" => Some(Self::S3),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cdn => "cdn",
            Self::S3 => "s3",
        }
    }
}

fn pick_latest(left: &str, right: Option<&str>) -> String {
    let Some(right) = right else {
        return left.to_string();
    };
    match (semver::Version::parse(left), semver::Version::parse(right)) {
        (Ok(left), Ok(right)) if right > left => right.to_string(),
        _ => left.to_string(),
    }
}

pub async fn run(
    check: bool,
    force: bool,
    json_output: bool,
    source: UpdateSource,
) -> Result<SelfUpdateResult> {
    let current = current_version().to_string();
    let platform = platform::current_platform();
    let client = client()?;
    let target = std::env::current_exe().context("获取当前 dytes 路径失败")?;

    let mut cdn_config: Option<DytesConfig> = None;
    let mut cdn_latest: Option<String> = None;
    let mut cdn_manifest_version: Option<String> = None;
    let mut s3_config: Option<DytesConfig> = None;
    let mut s3_latest: Option<String> = None;
    let mut s3_manifest_version: Option<String> = None;

    // S3 无 CDN 缓存，作为版本权威来源；除非显式只走 CDN。
    if source != UpdateSource::Cdn {
        match fetch_s3_manifest().await {
            Ok(config) => {
                let manifest_version = config.version.clone();
                let release_version = fetch_s3_release_version().await;
                let latest = pick_latest(&manifest_version, release_version.as_deref());
                println!("  S3 版本: v{latest}（清单 v{manifest_version}）");
                s3_manifest_version = Some(manifest_version);
                s3_latest = Some(latest);
                s3_config = Some(config);
            }
            Err(error) => {
                if source == UpdateSource::S3 {
                    return Err(error).context(
                        "S3 不可用；请检查 dytes config 中的 S3 凭证或 DYTES_S3_* 环境变量",
                    );
                }
                println!("  S3 不可用（{error:#}），回退 CDN");
            }
        }
    }

    // 网站 / 安装脚本使用 CDN；自动更新时 CDN 只作为下载加速来源。
    if source != UpdateSource::S3 {
        let release_version = fetch_release_version(&client).await;
        // 优先读取版本化清单：未版本化 dytes.json 可能被 CDN 缓存成旧版本。
        let config_result = match release_version.as_deref() {
            Some(version) => match fetch_versioned_manifest(&client, version).await {
                Ok(config) => Ok(config),
                Err(error) => {
                    println!("  版本化清单不可用（{error:#}），回退未版本化清单");
                    fetch_manifest(&client).await
                }
            },
            None => fetch_manifest(&client).await,
        };
        match config_result {
            Ok(config) => {
                let manifest_version = config.version.clone();
                let latest = pick_latest(&manifest_version, release_version.as_deref());
                if let Some(release) = &release_version {
                    if release != &manifest_version {
                        println!(
                            "  警告: dytes.json 版本 v{} 与 latest/version.js v{} 不一致，取较高版本",
                            manifest_version, release
                        );
                    }
                }
                println!("  CDN 版本: v{latest}（清单 v{manifest_version}）");
                cdn_manifest_version = Some(manifest_version);
                cdn_latest = Some(latest);
                cdn_config = Some(config);
            }
            Err(error) => {
                if source == UpdateSource::Cdn {
                    return Err(error);
                }
                println!("  CDN 不可用（{error:#}），仅使用 S3 版本");
            }
        }
    }

    let latest = match (&s3_latest, &cdn_latest) {
        (Some(s3), Some(cdn)) => pick_latest(cdn, Some(s3)),
        (Some(s3), None) => s3.clone(),
        (None, Some(cdn)) => cdn.clone(),
        (None, None) => bail!("无法从 S3 或 CDN 获取 dytes 最新版本"),
    };
    let version_source = if s3_latest.is_some() { "s3" } else { "cdn" };

    let current_semver = semver::Version::parse(&current).context("当前版本不是合法 semver")?;
    let latest_semver = semver::Version::parse(&latest).context("线上版本不是合法 semver")?;
    let has_update = latest_semver > current_semver;

    if check || (!has_update && !force) {
        let value = json!({
            "current": current,
            "latest": latest,
            "manifest_version": s3_manifest_version.or(cdn_manifest_version),
            "version_source": version_source,
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

    let (downloaded, used_source) = if source == UpdateSource::S3 {
        let config = s3_config
            .as_ref()
            .or(cdn_config.as_ref())
            .context("缺少 dytes 清单，无法确定二进制文件名")?;
        let binary = binary_name(config, &platform)?;
        let version = s3_latest.as_deref().unwrap_or(&latest);
        let key = download_s3_binary(version, &platform, &binary, &temp).await?;
        verify_s3_checksum(version, &platform, &binary, &temp).await?;
        (key, "s3")
    } else if let Some(config) = cdn_config.as_ref() {
        let binary = binary_name(config, &platform)?;
        let versioned_url = download_url(&latest, &platform, &binary);
        println!("  下载: {versioned_url}");
        match download_file(&client, &versioned_url, &temp).await {
            Ok(()) => {
                if let Err(error) = verify_checksum(&client, &versioned_url, &temp).await {
                    let _ = fs::remove_file(&temp);
                    return Err(error);
                }
                (versioned_url, "cdn")
            }
            Err(cdn_error) => {
                if source == UpdateSource::Cdn {
                    let _ = fs::remove_file(&temp);
                    bail!("CDN 下载失败: {cdn_error}");
                }
                println!("  CDN 下载目标版本失败（{cdn_error}），改用 S3");
                let config = s3_config
                    .as_ref()
                    .or(cdn_config.as_ref())
                    .context("缺少 dytes 清单，无法确定二进制文件名")?;
                let binary = binary_name(config, &platform)?;
                let version = s3_latest.as_deref().unwrap_or(&latest);
                let _ = fs::remove_file(&temp);
                let key = download_s3_binary(version, &platform, &binary, &temp).await?;
                verify_s3_checksum(version, &platform, &binary, &temp).await?;
                (key, "s3")
            }
        }
    } else {
        println!("  没有可用的 CDN 清单，改用 S3");
        let config = s3_config
            .as_ref()
            .context("缺少 dytes 清单，无法确定二进制文件名")?;
        let binary = binary_name(config, &platform)?;
        let version = s3_latest.as_deref().unwrap_or(&latest);
        let key = download_s3_binary(version, &platform, &binary, &temp).await?;
        verify_s3_checksum(version, &platform, &binary, &temp).await?;
        (key, "s3")
    };

    set_executable(&temp)?;
    replace_binary(&temp, &target)?;

    let value = json!({
        "current": current,
        "latest": latest,
        "platform": platform,
        "updated": true,
        "source": used_source,
        "download_path": downloaded,
        "path": target,
    });
    if json_output {
        println!("{value}");
    } else {
        println!(
            "dytes 已更新到 v{latest}（{used_source}）: {}",
            target.display()
        );
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
    fn parses_release_version_script() {
        let script = r#"window.DYTES_RELEASE={"version":"0.1.11","downloads":{"windows":"..."}};"#;
        assert_eq!(
            parse_release_version_script(script).as_deref(),
            Some("0.1.11")
        );
        assert_eq!(parse_release_version_script("not js"), None);
    }

    #[test]
    fn parses_download_source() {
        assert_eq!(UpdateSource::parse("auto"), Some(UpdateSource::Auto));
        assert_eq!(UpdateSource::parse("cdn"), Some(UpdateSource::Cdn));
        assert_eq!(UpdateSource::parse("s3"), Some(UpdateSource::S3));
        assert_eq!(UpdateSource::parse("api"), None);
        assert_eq!(UpdateSource::parse("ftp"), None);
    }

    #[test]
    fn picks_the_higher_version() {
        assert_eq!(pick_latest("0.1.10", Some("0.1.11")), "0.1.11");
        assert_eq!(pick_latest("0.1.11", Some("0.1.10")), "0.1.11");
        assert_eq!(pick_latest("0.1.11", None), "0.1.11");
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

    #[test]
    fn builds_versioned_url() {
        assert!(
            download_url("0.1.11", "windows", "dytes.exe").ends_with("/0.1.11/windows/dytes.exe")
        );
    }
}
