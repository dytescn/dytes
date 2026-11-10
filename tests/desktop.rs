//! 桌面快捷方式测试：用 piksel 命名的可执行文件验证映射结果。
//!
//! 测试写入 `.dytes/.../Desktop`，不触碰真实桌面。

mod common;

use common::TestEnv;
use dytes::utils::desktop;

#[test]
fn creates_piksel_desktop_shortcut() {
    let env = TestEnv::new();
    let desktop_dir = env.root().join("Desktop");
    let piksel = env.root().join("piksel.exe");

    std::fs::copy(env!("CARGO_BIN_EXE_dytes"), &piksel).expect("复制 piksel 可执行文件");

    let shortcut =
        desktop::create_shortcut_in(&desktop_dir, "piksel", &piksel).expect("创建桌面快捷方式");

    assert!(shortcut.is_file(), "快捷方式不存在: {}", shortcut.display());
    assert!(
        std::fs::metadata(&shortcut).expect("读取快捷方式").len() > 0,
        "快捷方式为空: {}",
        shortcut.display()
    );

    #[cfg(windows)]
    {
        assert_eq!(
            shortcut.extension().and_then(|value| value.to_str()),
            Some("lnk")
        );

        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
                 $ws = New-Object -ComObject WScript.Shell; \
                 $ws.CreateShortcut($env:DYTES_LNK).TargetPath",
            ])
            .env("DYTES_LNK", &shortcut)
            .output()
            .expect("读取快捷方式目标");

        assert!(
            output.status.success(),
            "读取目标失败: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let target = String::from_utf8_lossy(&output.stdout)
            .replace('\0', "")
            .to_lowercase();
        assert!(
            target.contains("piksel.exe"),
            "快捷方式目标不正确: {target}"
        );
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        assert_eq!(
            shortcut.extension().and_then(|value| value.to_str()),
            Some("desktop")
        );
        let content = std::fs::read_to_string(&shortcut).expect("读取启动项");
        assert!(content.contains("piksel.exe"));
    }
}

#[test]
fn rejects_empty_app_name() {
    let env = TestEnv::new();
    let piksel = env.root().join("piksel.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_dytes"), &piksel).expect("复制 piksel 可执行文件");

    let error = desktop::create_shortcut_in(&env.root().join("Desktop"), "   ", &piksel)
        .expect_err("空应用名应失败");
    assert!(error.to_string().contains("应用名为空"));
}

#[test]
fn rejects_missing_app() {
    let env = TestEnv::new();
    let missing = env.root().join("missing.exe");

    let error = desktop::create_shortcut_in(&env.root().join("Desktop"), "piksel", &missing)
        .expect_err("不存在的应用应失败");
    assert!(error.to_string().contains("应用不存在"));
}
