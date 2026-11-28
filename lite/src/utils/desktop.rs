//! 桌面快捷方式：把已安装的应用映射到桌面，双击即可运行。
//!
//! - Windows：创建 `.lnk` 快捷方式（通过 PowerShell 的 `WScript.Shell`）。
//! - Linux：创建 `.desktop` 启动项。
//! - macOS：创建 `.command` 启动脚本。
//!
//! 桌面目录默认取系统桌面，可用 `DYTES_DESKTOP_DIR` 覆盖（便于测试）。

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// 系统桌面目录，支持 `DYTES_DESKTOP_DIR` 覆盖。
pub fn desktop_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("DYTES_DESKTOP_DIR") {
        let dir = PathBuf::from(dir);
        if !dir.as_os_str().is_empty() {
            return Ok(dir);
        }
    }

    dirs::desktop_dir().ok_or_else(|| anyhow::anyhow!("无法获取桌面目录"))
}

/// 在系统桌面为应用创建快捷方式，返回快捷方式路径。
pub fn create_app_shortcut(display_name: &str, app_path: &Path) -> Result<PathBuf> {
    create_shortcut_in(&desktop_dir()?, display_name, app_path)
}

/// 在指定桌面目录创建快捷方式（测试或自定义桌面目录时使用）。
pub fn create_shortcut_in(desktop: &Path, display_name: &str, app_path: &Path) -> Result<PathBuf> {
    let label = safe_label(display_name)?;

    if !app_path.exists() {
        bail!("应用不存在，无法创建桌面快捷方式: {}", app_path.display());
    }
    let target = std::path::absolute(app_path)
        .with_context(|| format!("解析应用路径失败: {}", app_path.display()))?;

    fs::create_dir_all(desktop)
        .with_context(|| format!("创建桌面目录失败: {}", desktop.display()))?;

    create_shortcut(&label, &target, desktop)
}

/// 生成安全的应用标签：首字母大写，并去掉路径分隔符与 Windows 非法字符。
fn safe_label(display_name: &str) -> Result<String> {
    let trimmed = display_name.trim();
    if trimmed.is_empty() {
        bail!("应用名为空，无法创建桌面快捷方式");
    }

    let mut chars = trimmed.chars();
    let first = chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_default();
    let label = format!("{first}{}", chars.as_str());

    Ok(label.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "-"))
}

#[cfg(windows)]
fn create_shortcut(label: &str, target: &Path, desktop: &Path) -> Result<PathBuf> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let shortcut = desktop.join(format!("{label}.lnk"));
    let working_dir = target.parent().unwrap_or(desktop);

    let script = "\
        $ws = New-Object -ComObject WScript.Shell; \
        $sc = $ws.CreateShortcut($env:DYTES_LNK); \
        $sc.TargetPath = $env:DYTES_TARGET; \
        $sc.WorkingDirectory = $env:DYTES_WORKDIR; \
        $sc.IconLocation = $env:DYTES_TARGET; \
        $sc.Save()";

    let mut command = std::process::Command::new("powershell");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("DYTES_LNK", &shortcut)
        .env("DYTES_TARGET", target)
        .env("DYTES_WORKDIR", working_dir)
        .creation_flags(CREATE_NO_WINDOW);

    let output = command
        .output()
        .context("调用 PowerShell 创建快捷方式失败")?;
    if !output.status.success() {
        bail!(
            "创建快捷方式失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    if !shortcut.is_file() {
        bail!("快捷方式未生成: {}", shortcut.display());
    }

    Ok(shortcut)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn create_shortcut(label: &str, target: &Path, desktop: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let shortcut = desktop.join(format!("{label}.desktop"));
    let content = format!(
        "[Desktop Entry]\nType=Application\nName={label}\nExec=\"{}\"\nTerminal=false\n",
        target.display()
    );
    fs::write(&shortcut, content)
        .with_context(|| format!("写入桌面启动项失败: {}", shortcut.display()))?;
    fs::set_permissions(&shortcut, fs::Permissions::from_mode(0o755))?;
    Ok(shortcut)
}

#[cfg(target_os = "macos")]
fn create_shortcut(label: &str, target: &Path, desktop: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let shortcut = desktop.join(format!("{label}.command"));
    let content = format!("#!/bin/sh\nexec \"{}\"\n", target.display());
    fs::write(&shortcut, content)
        .with_context(|| format!("写入桌面启动脚本失败: {}", shortcut.display()))?;
    fs::set_permissions(&shortcut, fs::Permissions::from_mode(0o755))?;
    Ok(shortcut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_and_capitalizes_label() {
        assert_eq!(safe_label("piksel").unwrap(), "Piksel");
        assert_eq!(safe_label(" demo/app ").unwrap(), "Demo-app");
        assert!(safe_label("   ").is_err());
    }
}
