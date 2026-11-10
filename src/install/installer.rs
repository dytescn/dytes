use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::update::loader::fetch_text;
use crate::utils::dytes::DytesConfig;

/// 安装器负责分发的应用名。
///
/// 远程配置位于 `app/{APP_NAME}/dytes.json`，安装完成后写入安装目录，
/// 供内置更新引擎读取依赖列表。
pub const APP_NAME: &str = "piksel";

/// 安装进度：total 为整体进度（0~1），file 为当前文件进度（0~1）
#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub total: f32,
    pub file: f32,
    pub status: String,
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
/// 2. 下载应用清单 `app/{APP_NAME}/dytes.json` 并写入安装目录
/// 3. 在安装目录运行内置更新引擎（等价 `dytes update --auto`）
/// 4. 返回应用可执行文件路径（不自动启动）
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

    // 1. 获取应用配置并写入安装目录
    let app_config = runtime.block_on(async {
        log.write(&format!("获取应用配置: app/{}/dytes.json", APP_NAME));
        report(InstallProgress {
            total: 0.1,
            file: 0.0,
            status: "获取应用配置...".to_string(),
        });

        let app_text = fetch_text(&format!("app/{}/dytes.json", APP_NAME))
            .await
            .context("获取应用配置失败")?;
        log.write(&format!("应用配置: {}", app_text));

        let app_config: DytesConfig =
            serde_json::from_str(&app_text).context("解析应用配置失败")?;
        if app_config.name.trim().is_empty() {
            bail!("应用配置缺少 name 字段");
        }

        let config_path = install_dir.join("dytes.json");
        std::fs::write(&config_path, &app_text)
            .with_context(|| format!("写入 {} 失败", config_path.display()))?;
        log.write(&format!("应用配置已写入: {}", config_path.display()));

        report(InstallProgress {
            total: 0.2,
            file: 1.0,
            status: "配置准备完成".to_string(),
        });

        Ok::<_, anyhow::Error>(app_config)
    })?;

    // 2. 在安装目录运行内置更新引擎，不再依赖外部引导程序
    log.write("开始执行内置更新引擎（等价 dytes update --auto）");
    report(InstallProgress {
        total: 0.5,
        file: 0.0,
        status: "正在安装应用与依赖...".to_string(),
    });

    let update_result = {
        let _cwd_guard = CwdGuard::switch_to(install_dir)?;
        runtime.block_on(crate::update::dytes::init_dytes_with_options(None, true))
    };
    update_result.context("安装应用与依赖失败")?;

    // 3. 计算应用路径
    let app_exe = app_executable_name(&app_config.name);
    let app_path = install_dir.join(app_exe);
    log.write(&format!("安装完成，应用路径: {}", app_path.display()));

    // 4. 在桌面创建快捷方式；失败只提示，不影响安装结果
    match crate::utils::desktop::create_app_shortcut(&app_config.name, &app_path) {
        Ok(path) => log.write(&format!("桌面快捷方式: {}", path.display())),
        Err(error) => {
            log.write(&format!("创建桌面快捷方式失败: {error:#}"));
            eprintln!("提示: 创建桌面快捷方式失败: {error:#}");
        }
    }

    report(InstallProgress {
        total: 1.0,
        file: 1.0,
        status: "安装完成".to_string(),
    });

    Ok(app_path)
}

/// 临时切换工作目录，离开作用域时恢复。
struct CwdGuard(Option<PathBuf>);

impl CwdGuard {
    fn switch_to(dir: &Path) -> Result<Self> {
        let previous = std::env::current_dir().ok();
        std::env::set_current_dir(dir)
            .with_context(|| format!("切换目录失败: {}", dir.display()))?;
        Ok(Self(previous))
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            let _ = std::env::set_current_dir(previous);
        }
    }
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
    fn builds_app_executable_name() {
        let name = app_executable_name("piksel");
        if cfg!(windows) {
            assert_eq!(name, "piksel.exe");
        } else {
            assert_eq!(name, "piksel");
        }
    }
}
