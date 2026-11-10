//! 可执行程序的平台后缀与命令入口。
//!
//! 发布产物按平台命名：Windows `.exe`、Linux `.bin`、macOS `.app`；
//! Linux / macOS 上额外创建无后缀命令入口（例如 `dytes.bin` → `dytes`），
//! 使命令行始终使用 `dytes`，不带平台后缀。

use anyhow::Result;
#[cfg(unix)]
use anyhow::Context;
#[cfg(unix)]
use std::fs;
use std::path::Path;

/// 去掉发布产物的平台后缀，得到命令入口名。
///
/// `.exe` 由 Windows 自身解析，不需要别名；`.bin` / `.app` 返回无后缀名。
pub fn command_alias(filename: &str) -> Option<&str> {
    filename
        .strip_suffix(".bin")
        .or_else(|| filename.strip_suffix(".app"))
}

/// 为 `.bin` / `.app` 产物创建无后缀命令入口（Unix）。
#[cfg(unix)]
pub fn install_command_alias(dir: &Path, filename: &str) -> Result<()> {
    let Some(alias) = command_alias(filename) else {
        return Ok(());
    };
    let target = dir.join(filename);
    if !target.is_file() {
        return Ok(());
    }
    let link = dir.join(alias);
    let _ = fs::remove_file(&link);
    std::os::unix::fs::symlink(&target, &link)
        .with_context(|| format!("创建命令入口失败: {}", link.display()))?;

    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(&target)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&target, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
pub fn install_command_alias(_dir: &Path, _filename: &str) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_platform_suffixes() {
        assert_eq!(command_alias("dytes.bin"), Some("dytes"));
        assert_eq!(command_alias("cyctron.app"), Some("cyctron"));
        assert_eq!(command_alias("dytes.exe"), None);
        assert_eq!(command_alias("database.dll"), None);
    }
}
