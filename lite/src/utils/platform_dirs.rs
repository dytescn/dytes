//! 轻量目录解析，替代 `dirs` crate。
//!
//! 原因：`dirs` 6 依赖 `dirs-sys`，后者在 Windows 上调用
//! `windows::Win32::System::Com::CoTaskMemFree`，会把 `combase.dll` 写进导入表，
//! 导致 Windows 7 无法启动。这里只用环境变量与 `PathBuf`，不依赖 COM。

use std::env;
use std::path::PathBuf;

pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
    }
}

pub fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        home_dir().map(|home| home.join("Library").join("Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .or_else(|| home_dir().map(|home| home.join(".config")))
    }
}

pub fn data_local_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        home_dir().map(|home| home.join("Library").join("Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .or_else(|| home_dir().map(|home| home.join(".local").join("share")))
    }
}

pub fn desktop_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        home_dir().map(|home| home.join("Desktop"))
    }
    #[cfg(target_os = "macos")]
    {
        home_dir().map(|home| home.join("Desktop"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        env::var_os("XDG_DESKTOP_DIR")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .or_else(|| home_dir().map(|home| home.join("Desktop")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_absolute_paths_when_env_is_set() {
        // 只验证函数在环境变量存在时返回非空路径；不依赖具体机器配置。
        if env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).is_some() {
            assert!(home_dir().is_some());
        }
    }
}
