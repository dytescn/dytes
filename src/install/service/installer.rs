use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::install::utils::s3;

/// 安装器负责分发的应用名。
///
/// 远程配置位于 `app/{APP_NAME}/dytes.json`，安装完成后该文件会被放到
/// 安装目录，供 `update.exe` 读取依赖列表。
pub const APP_NAME: &str = "piksel";

/// update 包在 S3 上的包名（`core/update/...`）
const UPDATE_PACKAGE: &str = "update";

/// 安装进度：total 为整体进度（0~1），file 为当前文件进度（0~1）
#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub total: f32,
    pub file: f32,
    pub status: String,
}

#[derive(Debug, Deserialize)]
struct PackageConfig {
    name: String,
    version: String,
    #[serde(default)]
    exports: HashMap<String, String>,
}

/// 安装日志，写入安装目录下的 install.log
struct InstallLog {
    file: Option<File>,
}

impl InstallLog {
    fn new(path: &Path) -> Self {
        let file = File::create(path).ok();
        let mut log = Self { file };
        log.write(&format!("==== Piksel 安装日志 {} ====", timestamp_secs()));
        log
    }

    fn write(&mut self, message: &str) {
        if let Some(file) = &mut self.file {
            let _ = writeln!(file, "{}", message);
            let _ = file.flush();
        }
    }
}

fn timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 执行完整安装流程：
/// 1. 创建安装目录
/// 2. 下载 update 包的 `update.exe`
/// 3. 下载应用的 `dytes.json`
/// 4. 在安装目录执行 `update.exe --auto`
/// 5. 返回应用可执行文件路径（不自动启动）
pub fn run_install<F>(install_dir: &Path, mut report: F) -> Result<PathBuf>
where
    F: FnMut(InstallProgress),
{
    std::fs::create_dir_all(install_dir)
        .with_context(|| format!("创建安装目录失败: {}", install_dir.display()))?;

    let mut log = InstallLog::new(&install_dir.join("install.log"));
    log.write(&format!("安装目录: {}", install_dir.display()));

    report(InstallProgress {
        total: 0.0,
        file: 0.0,
        status: "准备安装...".to_string(),
    });

    let runtime = tokio::runtime::Runtime::new().context("创建异步运行时失败")?;

    let (update_exe, app_config) = runtime.block_on(async {
        // 1. 获取 update 包信息
        log.write("获取 update 包版本信息: core/update/dytes.json");
        report(InstallProgress {
            total: 0.02,
            file: 0.0,
            status: "获取 update 版本信息...".to_string(),
        });

        let update_text = s3::fetch_text(&format!("core/{}/dytes.json", UPDATE_PACKAGE))
            .await
            .context("获取 update 包配置失败")?;
        log.write(&format!("update 配置: {}", update_text));

        let update_config: PackageConfig =
            serde_json::from_str(&update_text).context("解析 update 包配置失败")?;

        if update_config.exports.is_empty() {
            bail!("update 包 {} 没有导出文件", update_config.version);
        }

        // 2. 下载 update 包导出的文件（通常是 update.exe）
        let mut files: Vec<(String, String)> = update_config
            .exports
            .iter()
            .map(|(source, dest)| (source.clone(), dest.clone()))
            .collect();
        files.sort_by(|a, b| a.1.cmp(&b.1));

        let download_start = 0.05f32;
        let download_end = 0.45f32;
        let span = (download_end - download_start) / files.len() as f32;

        let mut update_exe: Option<PathBuf> = None;

        for (index, (_source, remote_name)) in files.iter().enumerate() {
            let filename = file_name_of(remote_name);
            let dest = install_dir.join(&filename);
            let key = format!(
                "core/{}/{}/{}",
                UPDATE_PACKAGE, update_config.version, filename
            );
            let base = download_start + span * index as f32;

            log.write(&format!("下载 {} -> {}", key, dest.display()));
            report(InstallProgress {
                total: base,
                file: 0.0,
                status: format!("下载 {}...", filename),
            });

            let mut last_percent = -1i32;
            s3::download_to(&key, &dest, |done, total| {
                let Some(percent) = done.saturating_mul(100).checked_div(total) else {
                    return;
                };
                let percent = (percent as i32).clamp(0, 100);
                if percent != last_percent {
                    last_percent = percent;
                    report(InstallProgress {
                        total: base + span * (percent as f32 / 100.0),
                        file: percent as f32 / 100.0,
                        status: format!("下载 {}... {}%", filename, percent),
                    });
                }
            })
            .await
            .with_context(|| format!("下载 {} 失败", key))?;

            log.write(&format!("下载完成: {}", dest.display()));
            report(InstallProgress {
                total: base + span,
                file: 1.0,
                status: format!("{} 下载完成", filename),
            });

            if update_exe.is_none() {
                update_exe = Some(dest);
            }
        }

        let update_exe = update_exe.ok_or_else(|| anyhow!("update 包没有可执行文件"))?;

        // 3. 下载应用配置 dytes.json
        log.write(&format!("获取应用配置: app/{}/dytes.json", APP_NAME));
        report(InstallProgress {
            total: 0.48,
            file: 0.0,
            status: "获取应用配置...".to_string(),
        });

        let app_text = s3::fetch_text(&format!("app/{}/dytes.json", APP_NAME))
            .await
            .context("获取应用配置失败")?;
        log.write(&format!("应用配置: {}", app_text));

        let app_config: PackageConfig =
            serde_json::from_str(&app_text).context("解析应用配置失败")?;
        if app_config.name.trim().is_empty() {
            bail!("应用配置缺少 name 字段");
        }

        let config_path = install_dir.join("dytes.json");
        std::fs::write(&config_path, &app_text)
            .with_context(|| format!("写入 {} 失败", config_path.display()))?;
        log.write(&format!("应用配置已写入: {}", config_path.display()));

        report(InstallProgress {
            total: 0.5,
            file: 1.0,
            status: "配置准备完成".to_string(),
        });

        Ok::<_, anyhow::Error>((update_exe, app_config))
    })?;

    // 4. 在安装目录运行 update.exe --auto
    log.write(&format!(
        "开始执行: {} --auto (工作目录: {})",
        update_exe.display(),
        install_dir.display()
    ));
    report(InstallProgress {
        total: 0.5,
        file: 0.0,
        status: "正在安装应用与依赖...".to_string(),
    });

    run_update(&update_exe, install_dir, &mut log, &mut report)?;

    // 5. 计算应用路径
    let app_exe = app_executable_name(&app_config.name);
    let app_path = install_dir.join(app_exe);
    log.write(&format!("安装完成，应用路径: {}", app_path.display()));

    report(InstallProgress {
        total: 1.0,
        file: 1.0,
        status: "安装完成".to_string(),
    });

    Ok(app_path)
}

/// 启动已安装的应用
pub fn launch_app(app_path: &Path) -> Result<()> {
    if !app_path.exists() {
        bail!("应用不存在: {}", app_path.display());
    }

    let mut command = Command::new(app_path);
    if let Some(dir) = app_path.parent() {
        command.current_dir(dir);
    }

    command
        .spawn()
        .with_context(|| format!("启动应用失败: {}", app_path.display()))?;

    Ok(())
}

fn run_update<F>(
    update_exe: &Path,
    install_dir: &Path,
    log: &mut InstallLog,
    report: &mut F,
) -> Result<()>
where
    F: FnMut(InstallProgress),
{
    let mut command = Command::new(update_exe);
    command
        .arg("--auto")
        .current_dir(install_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // 安装过程中不要弹出 update.exe 的控制台窗口
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .with_context(|| format!("启动更新程序失败: {}", update_exe.display()))?;

    let stderr_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let mut tail = Vec::new();
            let reader = BufReader::new(stderr);
            for line in reader.split(b'\n') {
                match line {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).trim().to_string();
                        if !text.is_empty() {
                            tail.push(text);
                            if tail.len() > 40 {
                                tail.remove(0);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            tail
        })
    });

    if let Some(stdout) = child.stdout.take() {
        let mut reader = BufReader::new(stdout);
        let mut buffer = Vec::new();
        let mut last_status = String::new();

        loop {
            buffer.clear();
            let read = reader
                .read_until(b'\n', &mut buffer)
                .context("读取更新程序输出失败")?;
            if read == 0 {
                break;
            }

            // update 的下载进度使用 \r 覆盖同一行，这里按 \r 拆分后逐段解析
            for segment in String::from_utf8_lossy(&buffer).split('\r') {
                let segment = segment.trim();
                if segment.is_empty() {
                    continue;
                }

                log.write(segment);

                if let Some(percent) = parse_total_progress(segment) {
                    let status = segment
                        .split('-')
                        .nth(1)
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|| "正在安装...".to_string());
                    if status != last_status {
                        last_status = status.clone();
                        report(InstallProgress {
                            total: 0.5 + percent / 100.0 * 0.5,
                            file: percent / 100.0,
                            status,
                        });
                    }
                } else if let Some(percent) = parse_download_progress(segment) {
                    report(InstallProgress {
                        total: 0.5 + percent / 100.0 * 0.5,
                        file: percent / 100.0,
                        status: format!("下载组件... {}%", percent as i32),
                    });
                }
            }
        }
    }

    let status = child.wait().context("等待更新程序结束失败")?;

    let stderr_tail = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    for line in &stderr_tail {
        log.write(&format!("[stderr] {}", line));
    }

    if !status.success() {
        let detail = if stderr_tail.is_empty() {
            String::new()
        } else {
            format!("：{}", stderr_tail.join(" | "))
        };
        bail!(
            "更新程序执行失败（退出码 {}）{}",
            status.code().unwrap_or(-1),
            detail
        );
    }

    Ok(())
}

/// 匹配 update 输出的整体进度：`进度: 42.0% - 更新核心: xxx`
fn parse_total_progress(line: &str) -> Option<f32> {
    let rest = line.trim_start().strip_prefix("进度:")?;
    let percent = rest.split('%').next()?.trim().parse::<f32>().ok()?;
    Some(percent.clamp(0.0, 100.0))
}

/// 匹配 update 下载文件时输出的 `下载进度: 42.0%`
fn parse_download_progress(line: &str) -> Option<f32> {
    let rest = line.trim_start().strip_prefix("下载进度:")?;
    let percent = rest.split('%').next()?.trim().parse::<f32>().ok()?;
    Some(percent.clamp(0.0, 100.0))
}

fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn app_executable_name(app_name: &str) -> String {
    if cfg!(windows) {
        if app_name.to_lowercase().ends_with(".exe") {
            app_name.to_string()
        } else {
            format!("{}.exe", app_name)
        }
    } else {
        app_name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_total_progress_line() {
        assert_eq!(
            parse_total_progress("进度: 42.5% - 更新核心: web"),
            Some(42.5)
        );
        assert_eq!(parse_total_progress("  进度: 100.0% - 完成"), Some(100.0));
        assert_eq!(parse_total_progress("下载进度: 42.5%"), None);
        assert_eq!(parse_total_progress("随便一行"), None);
    }

    #[test]
    fn parses_download_progress_line() {
        assert_eq!(parse_download_progress("  下载进度: 12.0%"), Some(12.0));
        assert_eq!(parse_download_progress("进度: 12.0% - x"), None);
    }

    #[test]
    fn builds_app_executable_name() {
        let name = app_executable_name("piksel");
        if cfg!(windows) {
            assert_eq!(name, "piksel.exe");
        } else {
            assert_eq!(name, "piksel");
        }
    }
}
