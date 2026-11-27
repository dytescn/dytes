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
///
/// exFAT / FAT32 不支持符号链接（`symlink(2)` 返回 EPERM），此时依次回退为
/// shell 包装脚本、复制产物，避免命令入口创建失败中断整个安装 / 更新。
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

    // 产物本身需要执行位；exFAT / FAT32 上 chmod 无效，忽略错误继续。
    let _ = set_executable(&target);

    // 1) 符号链接：支持 symlink 的文件系统。
    if std::os::unix::fs::symlink(filename, &link).is_ok() {
        return Ok(());
    }

    // 2) exFAT / FAT32 等不支持符号链接：写入 shell 包装脚本。
    if write_wrapper_script(&link, filename).is_ok() {
        let _ = set_executable(&link);
        return Ok(());
    }

    // 3) 最后回退为复制；产物已通过校验，复制不改变内容。
    fs::copy(&target, &link)
        .with_context(|| format!("创建命令入口失败: {}", link.display()))?;
    let _ = set_executable(&link);
    Ok(())
}

/// 包装脚本内容：以自身所在目录定位真实产物，并透传参数。
#[cfg(any(unix, test))]
fn wrapper_script(filename: &str) -> String {
    format!("#!/bin/sh\nexec \"$(dirname \"$0\")/{filename}\" \"$@\"\n")
}

/// 写入无后缀命令入口的包装脚本（用于不支持符号链接的文件系统）。
#[cfg(unix)]
fn write_wrapper_script(link: &Path, filename: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o755)
        .open(link)?;
    file.write_all(wrapper_script(filename).as_bytes())?;
    file.sync_all()
}

/// 尽量给文件加执行位；仅 Unix 需要。
#[cfg(unix)]
fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
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

    #[test]
    fn wrapper_script_execs_sibling_binary() {
        assert_eq!(
            wrapper_script("cyctron.bin"),
            "#!/bin/sh\nexec \"$(dirname \"$0\")/cyctron.bin\" \"$@\"\n"
        );
    }
}
