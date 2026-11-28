//! `dytes setup`：把 dytes 安装到用户目录并写入用户 `PATH`。
//!
//! 双击 `dytes-windows.exe`（无参数、由资源管理器启动）时也进入同一流程，
//! 对应 `docs/requirements/windows-installer.md` 的 M1。
//!
//! 设计约束：安装器与主程序是同一个二进制，产物依赖 Windows 10+ 的系统 API，
//! 因此**旧系统上的提示只能放在下载页 / 安装脚本**（见 issues.md I-22），
//! 本模块不做版本检测。

#[cfg(any(windows, test))]
use std::path::Path;
use std::path::PathBuf;

#[cfg(not(windows))]
use anyhow::bail;
use anyhow::Result;

/// 安装选项。`interactive` 为真表示双击启动：失败时可弹窗询问 / 提示。
#[derive(Debug, Clone, Default)]
pub struct SetupOptions {
    pub dir: Option<PathBuf>,
    pub offline: bool,
    pub no_path: bool,
    pub json: bool,
    pub interactive: bool,
}

/// 安装结果。
#[derive(Debug, Clone)]
pub struct SetupOutcome {
    pub version: String,
    /// `online` / `bundled` / `inplace`
    pub source: &'static str,
    pub path: PathBuf,
    pub path_updated: bool,
    pub log_path: PathBuf,
}

#[derive(clap::Args, Debug)]
pub struct SetupArgs {
    /// 只安装当前 exe 内置版本，不访问网络
    #[arg(long)]
    pub offline: bool,
    /// 安装目录（默认 %LOCALAPPDATA%\Programs\dytes）
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,
    /// 不修改用户 PATH
    #[arg(long = "no-path")]
    pub no_path: bool,
    /// 输出 JSON
    #[arg(long)]
    pub json: bool,
}

pub fn run_cli(args: SetupArgs) -> crate::utils::error::Result<()> {
    let options = SetupOptions {
        dir: args.dir,
        offline: args.offline,
        no_path: args.no_path,
        json: args.json,
        interactive: false,
    };
    let outcome = run(&options).map_err(|error| {
        crate::utils::error::DytesError::failed(format!("安装 dytes 失败: {error:#}"))
    })?;
    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "version": outcome.version,
                "source": outcome.source,
                "path": outcome.path,
                "path_updated": outcome.path_updated,
                "log": outcome.log_path,
            })
        );
    } else {
        println!(
            "已安装 dytes v{} 到 {}",
            outcome.version,
            outcome.path.display()
        );
        if outcome.path_updated {
            println!("已加入用户 PATH；请打开新的终端后再运行 dytes");
        } else if !args.no_path {
            println!("用户 PATH 无需修改（已包含安装目录或写入失败，详见日志）");
        }
        println!("安装日志: {}", outcome.log_path.display());
        println!("验证: 新开 cmd 执行 dytes --version");
    }
    Ok(())
}

pub fn run(options: &SetupOptions) -> Result<SetupOutcome> {
    #[cfg(windows)]
    {
        windows::run_setup(options)
    }
    #[cfg(not(windows))]
    {
        let _ = options;
        bail!("dytes setup 目前仅支持 Windows；其它平台请使用发布产物或安装脚本")
    }
}

/// 是否由资源管理器双击启动（无参数 + 父进程 explorer.exe）。
pub fn should_run_installer() -> bool {
    #[cfg(windows)]
    {
        windows::is_double_click_launch()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 双击安装入口：隐藏控制台，成功 / 失败都用弹窗反馈。
pub fn run_double_click() -> i32 {
    #[cfg(windows)]
    {
        windows::run_double_click()
    }
    #[cfg(not(windows))]
    {
        1
    }
}

/// 把目录追加到 PATH 字符串；已存在（大小写不敏感、忽略尾部斜杠）时返回 `None`。
#[cfg(any(windows, test))]
fn append_path(existing: &str, dir: &Path) -> Option<String> {
    let entry = dir.to_string_lossy().to_string();
    let target = normalize_path_entry(&entry);
    if existing
        .split(';')
        .any(|item| normalize_path_entry(item) == target)
    {
        return None;
    }
    let trimmed = existing.trim_end_matches(';');
    if trimmed.trim().is_empty() {
        Some(entry)
    } else {
        Some(format!("{trimmed};{entry}"))
    }
}

/// PATH 项归一化：去空白、去引号、去尾部斜杠、转小写，用于去重比较。
#[cfg(any(windows, test))]
fn normalize_path_entry(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

#[cfg(windows)]
mod windows {
    use std::fs;
    use std::path::{Path, PathBuf};

    use anyhow::{bail, Context, Result};
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, LPARAM, WPARAM};
    use windows::Win32::System::Console::{
        FreeConsole, GetConsoleMode, GetStdHandle, CONSOLE_MODE, STD_OUTPUT_HANDLE,
    };
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_VALUE_TYPE,
    };
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, SendMessageTimeoutW, HWND_BROADCAST, IDYES, MB_ICONERROR, MB_ICONINFORMATION,
        MB_ICONQUESTION, MB_OK, MB_YESNO, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    use super::{append_path, SetupOptions, SetupOutcome};

    pub(super) fn run_setup(options: &SetupOptions) -> Result<SetupOutcome> {
        let log = Logger::new();
        log.line("==== dytes setup 开始 ====");

        let target_dir = options.dir.clone().unwrap_or_else(default_install_dir);
        fs::create_dir_all(&target_dir)
            .with_context(|| format!("创建安装目录失败: {}", target_dir.display()))?;
        let target = target_dir.join("dytes.exe");
        log.line(format!("安装目录: {}", target_dir.display()));

        let temp = target_dir.join(format!(".dytes-download.{}", std::process::id()));
        let _ = fs::remove_file(&temp);

        let same_binary = std::env::current_exe()
            .ok()
            .map(|current| same_path(&current, &target))
            .unwrap_or(false);

        let (version, source) = if same_binary {
            log.line("当前运行的就是目标位置的 dytes，跳过文件替换");
            (env!("CARGO_PKG_VERSION").to_string(), "inplace")
        } else if options.offline {
            let result = install_bundled(&temp, &log)?;
            install_file(&temp, &target)?;
            result
        } else {
            match download_latest(&temp, &log) {
                Ok(result) => {
                    install_file(&temp, &target)?;
                    result
                }
                Err(error) => {
                    log.line(format!("在线安装失败: {error:#}"));
                    let fallback = options.interactive
                        && confirm(&format!(
                            "在线安装失败：\n{error:#}\n\n是否改为安装当前版本 v{}？",
                            env!("CARGO_PKG_VERSION")
                        ));
                    if fallback {
                        let result = install_bundled(&temp, &log)?;
                        install_file(&temp, &target)?;
                        result
                    } else {
                        return Err(error).context("在线安装失败；可加 --offline 安装当前版本");
                    }
                }
            }
        };

        let path_updated = if options.no_path {
            log.line("按 --no-path 跳过 PATH 修改");
            false
        } else {
            match add_to_user_path(&target_dir) {
                Ok(updated) => {
                    log.line(if updated {
                        "已写入用户 PATH"
                    } else {
                        "用户 PATH 已包含安装目录，无需修改"
                    });
                    updated
                }
                Err(error) => {
                    log.line(format!("写入用户 PATH 失败: {error:#}"));
                    if options.interactive {
                        message(&format!(
                            "dytes 已安装到：\n{}\n\n但写入用户 PATH 失败：\n{error:#}\n\n请手动把该目录加入 PATH。",
                            target_dir.display()
                        ));
                    }
                    false
                }
            }
        };
        log.line("==== dytes setup 完成 ====");

        Ok(SetupOutcome {
            version,
            source,
            path: target,
            path_updated,
            log_path: log.path().to_path_buf(),
        })
    }

    pub(super) fn is_double_click_launch() -> bool {
        if std::env::args_os().len() != 1 {
            return false;
        }
        if !stdout_is_console() {
            return false;
        }
        parent_process_name()
            .map(|name| name.eq_ignore_ascii_case("explorer.exe"))
            .unwrap_or(false)
    }

    /// 标准输出是否为控制台（非重定向）；重定向时不触发双击安装。
    fn stdout_is_console() -> bool {
        unsafe {
            let Ok(handle) = GetStdHandle(STD_OUTPUT_HANDLE) else {
                return false;
            };
            let mut mode = CONSOLE_MODE(0);
            GetConsoleMode(handle, &mut mode).is_ok()
        }
    }

    pub(super) fn run_double_click() -> i32 {
        // CUI 子系统的 exe 双击时会分配控制台；立即释放，后续只用 MessageBox。
        let _ = unsafe { FreeConsole() };
        let options = SetupOptions {
            interactive: true,
            ..Default::default()
        };
        match super::run(&options) {
            Ok(outcome) => {
                message(&format!(
                    "dytes v{} 已安装到：\n{}\n\n请打开新的命令提示符（cmd）运行：\ndytes --version\n\n安装日志：{}",
                    outcome.version,
                    outcome.path.display(),
                    outcome.log_path.display()
                ));
                0
            }
            Err(error) => {
                message_error(&format!(
                    "安装失败：\n{error:#}\n\n安装日志：{}",
                    log_path().display()
                ));
                1
            }
        }
    }

    fn download_latest(temp: &Path, log: &Logger) -> Result<(String, &'static str)> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("创建异步运行时失败")?;
        runtime.block_on(async {
            use crate::update::self_update;

            let manifest = self_update::fetch_s3_manifest()
                .await
                .context("读取线上清单失败")?;
            let version = manifest.version.clone();
            let config = self_update::fetch_s3_versioned_manifest(&version)
                .await
                .unwrap_or(manifest);
            let binary = self_update::binary_name(&config, "windows")?;
            log.line(format!("在线版本: v{version}（{binary}）"));
            self_update::download_s3_binary(&version, "windows", &binary, temp).await?;
            self_update::verify_s3_checksum(&version, "windows", &binary, temp).await?;
            log.line("SHA-256 校验通过");
            Ok((version, "online"))
        })
    }

    fn install_bundled(temp: &Path, log: &Logger) -> Result<(String, &'static str)> {
        let current = std::env::current_exe().context("获取当前可执行文件失败")?;
        fs::copy(&current, temp)
            .with_context(|| format!("复制当前 dytes 失败: {}", current.display()))?;
        let version = env!("CARGO_PKG_VERSION").to_string();
        log.line(format!("使用内置版本: v{version}"));
        Ok((version, "bundled"))
    }

    fn install_file(temp: &Path, target: &Path) -> Result<()> {
        let backup = target.with_file_name("dytes.exe.old");
        let _ = fs::remove_file(&backup);
        let had_old = target.exists();
        if had_old {
            fs::rename(target, &backup).with_context(|| {
                format!(
                    "备份旧版本失败（文件可能正在运行，请关闭后重试）: {}",
                    target.display()
                )
            })?;
        }
        if let Err(error) = fs::rename(temp, target) {
            let _ = fs::remove_file(temp);
            if had_old {
                let _ = fs::rename(&backup, target);
            }
            return Err(error).with_context(|| format!("写入新版本失败: {}", target.display()));
        }
        if had_old {
            let _ = fs::remove_file(&backup);
        }
        Ok(())
    }

    fn same_path(left: &Path, right: &Path) -> bool {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }

    fn add_to_user_path(dir: &Path) -> Result<bool> {
        let subkey = HSTRING::from("Environment");
        let value = HSTRING::from("Path");
        let mut key = HKEY(std::ptr::null_mut());
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                &subkey,
                None,
                KEY_READ | KEY_WRITE,
                &mut key,
            )
        };
        if status.0 != 0 {
            bail!("打开注册表 HKCU\\Environment 失败（错误码 {}）", status.0);
        }

        let existing = read_string_value(key, &value);
        let updated = match existing
            .as_deref()
            .and_then(|current| append_path(current, dir))
        {
            Some(updated) => updated,
            None => {
                unsafe {
                    let _ = RegCloseKey(key);
                }
                return Ok(false);
            }
        };

        // REG_EXPAND_SZ 需要保留 %VAR% 并写入 UTF-16（含结尾 NUL）。
        let mut data = Vec::with_capacity(updated.len() * 2 + 2);
        for unit in updated.encode_utf16().chain(std::iter::once(0)) {
            data.extend_from_slice(&unit.to_le_bytes());
        }
        let status =
            unsafe { RegSetValueExW(key, &value, None, REG_EXPAND_SZ, Some(data.as_slice())) };
        unsafe {
            let _ = RegCloseKey(key);
        }
        if status.0 != 0 {
            bail!("写入用户 PATH 失败（错误码 {}）", status.0);
        }
        broadcast_environment_change();
        Ok(true)
    }

    fn read_string_value(key: HKEY, name: &HSTRING) -> Option<String> {
        let mut kind = REG_VALUE_TYPE(0);
        let mut size = 0u32;
        let status = unsafe {
            RegQueryValueExW(
                key,
                name,
                None,
                Some(&mut kind as *mut REG_VALUE_TYPE),
                None,
                Some(&mut size as *mut u32),
            )
        };
        if status.0 != 0 || size == 0 {
            return None;
        }

        let mut buffer = vec![0u8; size as usize + 2];
        let mut actual = size;
        let status = unsafe {
            RegQueryValueExW(
                key,
                name,
                None,
                Some(&mut kind as *mut REG_VALUE_TYPE),
                Some(buffer.as_mut_ptr()),
                Some(&mut actual as *mut u32),
            )
        };
        if status.0 != 0 {
            return None;
        }

        let units = (actual as usize / 2).min(buffer.len() / 2);
        let slice = unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u16, units) };
        let end = slice.iter().position(|&unit| unit == 0).unwrap_or(units);
        Some(String::from_utf16_lossy(&slice[..end]))
    }

    fn broadcast_environment_change() {
        let param = HSTRING::from("Environment");
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(param.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
    }

    fn default_install_dir() -> PathBuf {
        dirs::data_local_dir()
            .map(|dir| dir.join("Programs").join("dytes"))
            .unwrap_or_else(|| PathBuf::from("dytes"))
    }

    fn log_path() -> PathBuf {
        dirs::data_local_dir()
            .map(|dir| dir.join("dytes").join("logs").join("install.log"))
            .unwrap_or_else(|| PathBuf::from("dytes-install.log"))
    }

    struct Logger {
        path: PathBuf,
    }

    impl Logger {
        fn new() -> Self {
            let path = log_path();
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn line(&self, message: impl AsRef<str>) {
            use std::io::Write;

            let line = format!(
                "[{}] {}\n",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                message.as_ref()
            );
            if let Ok(mut file) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                let _ = file.write_all(line.as_bytes());
            }
        }
    }

    fn message(text: &str) {
        let text = HSTRING::from(text);
        let caption = HSTRING::from("dytes 安装");
        unsafe {
            MessageBoxW(None, &text, &caption, MB_OK | MB_ICONINFORMATION);
        }
    }

    fn message_error(text: &str) {
        let text = HSTRING::from(text);
        let caption = HSTRING::from("dytes 安装失败");
        unsafe {
            MessageBoxW(None, &text, &caption, MB_OK | MB_ICONERROR);
        }
    }

    fn confirm(text: &str) -> bool {
        let text = HSTRING::from(text);
        let caption = HSTRING::from("dytes 安装");
        let result = unsafe { MessageBoxW(None, &text, &caption, MB_YESNO | MB_ICONQUESTION) };
        result == IDYES
    }

    fn parent_process_name() -> Option<String> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
            let mut entry = PROCESSENTRY32W::default();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

            let mut processes: Vec<(u32, u32, String)> = Vec::new();
            let mut has_entry = Process32FirstW(snapshot, &mut entry).is_ok();
            while has_entry {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&unit| unit == 0)
                    .unwrap_or(entry.szExeFile.len());
                processes.push((
                    entry.th32ProcessID,
                    entry.th32ParentProcessID,
                    String::from_utf16_lossy(&entry.szExeFile[..len]),
                ));
                entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
                has_entry = Process32NextW(snapshot, &mut entry).is_ok();
            }
            let _ = CloseHandle(snapshot);

            let current = GetCurrentProcessId();
            let parent = processes
                .iter()
                .find(|(pid, _, _)| *pid == current)
                .map(|(_, parent, _)| *parent)?;
            processes
                .iter()
                .find(|(pid, _, _)| *pid == parent)
                .map(|(_, _, name)| name.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_path_adds_missing_directory() {
        assert_eq!(
            append_path(
                "C:\\Windows;C:\\Tools",
                Path::new("C:\\Users\\me\\Programs\\dytes")
            )
            .as_deref(),
            Some("C:\\Windows;C:\\Tools;C:\\Users\\me\\Programs\\dytes")
        );
    }

    #[test]
    fn append_path_is_idempotent_and_case_insensitive() {
        let dir = Path::new("C:\\Users\\me\\Programs\\dytes");
        assert_eq!(
            append_path("c:\\users\\ME\\programs\\DYTES\\;C:\\Windows", dir),
            None
        );
        assert_eq!(
            append_path("C:\\Windows;", dir).as_deref(),
            Some("C:\\Windows;C:\\Users\\me\\Programs\\dytes")
        );
    }

    #[test]
    fn append_path_handles_empty_value() {
        let dir = Path::new("C:\\dytes");
        assert_eq!(append_path("", dir).as_deref(), Some("C:\\dytes"));
        assert_eq!(append_path("   ", dir).as_deref(), Some("C:\\dytes"));
    }

    #[test]
    fn normalize_path_entry_trims_quotes_and_separators() {
        assert_eq!(normalize_path_entry("\"C:\\Tools\\\\\" "), "c:\\tools");
        assert_eq!(normalize_path_entry(""), "");
    }
}
