//! `dytes self-update`：从 packages.funxdata.com 检查并安装 CLI 自身的新版本。
//!
//! 与 `dytes update` 不同，本模块只处理 dytes 二进制本身，不读取当前目录的
//! `dytes.json`，也不更新应用依赖。下载走公开的 `core/dytes/latest/<platform>/`
//! 路径，并提供 SHA-256 校验与原子替换。

use crate::shared::config::DytesConfig;
use crate::shared::platform;
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MANIFEST_URL: &str = "https://packages.funxdata.com/core/dytes/dytes.json";
const DOWNLOAD_BASE: &str = "https://packages.funxdata.com/core/dytes";
const RETRIES: u32 = 3;

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
    let mut last_error = String::new();
    for attempt in 1..=RETRIES {
        match client.get(MANIFEST_URL).send().await {
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
    bail!("读取 {MANIFEST_URL} 失败: {last_error}")
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

fn download_url(version: &str, platform: &str, binary: &str, alias: bool) -> String {
    if alias {
        format!("{DOWNLOAD_BASE}/latest/{platform}/{binary}")
    } else {
        format!("{DOWNLOAD_BASE}/{version}/{platform}/{binary}")
    }
}

async fn download_file(client: &reqwest::Client, url: &str, dest: &Path) -> Result<()> {
    let mut last_error = String::new();
    for attempt in 1..=RETRIES {
        match client.get(url).send().await {
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

    let mut file =
        fs::File::open(file).with_context(|| format!("打开下载文件失败: {}", file.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).context("读取下载文件失败")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if !actual.eq_ignore_ascii_case(expected.trim()) {
        bail!("SHA-256 校验失败: 期望 {expected}，实际 {actual}");
    }
    println!("  SHA-256 校验通过");
    Ok(())
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
                    return Ok(());
                }
                Err(error) => {
                    let _ = fs::rename(&backup, target);
                    bail!("替换 dytes.exe 失败: {error}");
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
                return Ok(());
            }
        }
    }

    #[cfg(not(windows))]
    {
        fs::rename(temp, target).with_context(|| format!("替换 {} 失败", target.display()))?;
        Ok(())
    }
}

pub async fn run(check: bool, force: bool, json_output: bool) -> Result<SelfUpdateResult> {
    let current = current_version().to_string();
    let platform = platform::current_platform();
    let client = client()?;
    let config = fetch_manifest(&client).await?;
    let latest = config.version.clone();
    let current_semver = semver::Version::parse(&current).context("当前版本不是合法 semver")?;
    let latest_semver = semver::Version::parse(&latest).context("线上版本不是合法 semver")?;
    let has_update = latest_semver > current_semver;
    let target = std::env::current_exe().context("获取当前 dytes 路径失败")?;

    if check || (!has_update && !force) {
        let value = json!({
            "current": current,
            "latest": latest,
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

    let binary = binary_name(&config, &platform)?;
    let url = download_url(&latest, &platform, &binary, true);
    let temp = target.with_file_name(format!(
        ".{}.download.{}",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("dytes"),
        std::process::id()
    ));

    println!("更新 dytes: v{current} -> v{latest}（{platform}）");
    println!("  下载: {url}");
    let result = download_file(&client, &url, &temp).await;
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    if let Err(error) = verify_checksum(&client, &url, &temp).await {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    set_executable(&temp)?;
    replace_binary(&temp, &target)?;

    let value = json!({
        "current": current,
        "latest": latest,
        "platform": platform,
        "updated": true,
        "path": target,
    });
    if json_output {
        println!("{value}");
    } else {
        println!("dytes 已更新到 v{latest}: {}", target.display());
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
                    "linux": {"./target/release/dytes": "dytes"},
                    "macos": {"./target/release/dytes": "dytes"}
                }
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn picks_platform_binary() {
        let config = manifest();
        assert_eq!(binary_name(&config, "windows").unwrap(), "dytes.exe");
        assert_eq!(binary_name(&config, "linux").unwrap(), "dytes");
        assert_eq!(binary_name(&config, "macos").unwrap(), "dytes");
    }

    #[test]
    fn builds_latest_and_versioned_urls() {
        assert_eq!(
            download_url("0.1.11", "linux", "dytes", true),
            "https://packages.funxdata.com/core/dytes/latest/linux/dytes"
        );
        assert_eq!(
            download_url("0.1.11", "windows", "dytes.exe", false),
            "https://packages.funxdata.com/core/dytes/0.1.11/windows/dytes.exe"
        );
    }
}
