//! Win7 Lite spike（最小依赖集）：只验证工具链 + windows-sys + 常用 Rust 库
//! 能否生成不导入 Win8/10-only 符号的 PE。
//!
//! 不实际执行 Windows API 调用，用 black_box 保留函数引用，让链接器写入导入表。

use clap::Parser;
use sha2::Digest;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    version: bool,
}

fn main() {
    let cli = Cli::parse();
    if cli.version {
        println!("win7-min spike");
    }

    // 纯 Rust 依赖：确保它们被链接进产物（暴露传递依赖的导入表）。
    let _ = std::hint::black_box(serde_json::json!({"ok": true}));
    let _ = std::hint::black_box(chrono::Local::now().to_rfc3339());
    let _ = std::hint::black_box(uuid::Uuid::new_v4());
    let _ = std::hint::black_box(sha2::Sha256::digest(b"spike"));
    let _ = std::hint::black_box(semver::Version::parse("0.2.30").unwrap());
    let _ = std::hint::black_box(futures_util::future::ready(1u8));
    let _ = std::hint::black_box(toml::to_string(&std::collections::BTreeMap::<String, String>::new()));
    let _ = std::hint::black_box(dirs::home_dir());
    let _ = std::hint::black_box(rpassword::prompt_password("spike"));
    let _ = std::hint::black_box(anyhow::anyhow!("spike"));

    #[cfg(windows)]
    windows_api_refs();

    println!("win7-min ok");
}

#[cfg(windows)]
fn windows_api_refs() {
    use windows_sys::Win32::System::Console::{GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW;

    // 用函数指针引用保留导入；不实际调用，避免 CI 无桌面时阻塞。
    std::hint::black_box(GetConsoleMode as *const ());
    std::hint::black_box(GetStdHandle as *const ());
    std::hint::black_box(STD_OUTPUT_HANDLE);
    std::hint::black_box(CreateToolhelp32Snapshot as *const ());
    std::hint::black_box(Process32FirstW as *const ());
    std::hint::black_box(Process32NextW as *const ());
    std::hint::black_box(TH32CS_SNAPPROCESS);
    std::hint::black_box(RegOpenKeyExW as *const ());
    std::hint::black_box(RegQueryValueExW as *const ());
    std::hint::black_box(RegSetValueExW as *const ());
    std::hint::black_box(RegCloseKey as *const ());
    std::hint::black_box(MessageBoxW as *const ());
}
