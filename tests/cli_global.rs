//! 全局 CLI 行为：`--version`、`--help` 与非法命令。

mod common;

use common::{assert_code, stderr, stdout, TestEnv};

#[test]
fn version_flag_prints_package_version() {
    let env = TestEnv::new();
    let out = env.run(&["--version"]);
    assert_code(&out, 0);
    assert!(stdout(&out).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_lists_all_commands() {
    let env = TestEnv::new();
    let out = env.run(&["--help"]);
    assert_code(&out, 0);

    let text = stdout(&out);
    for command in [
        "install",
        "update",
        "self-update",
        "publish",
        "check",
        "init",
        "config",
    ] {
        assert!(text.contains(command), "帮助缺少命令: {command}\n{text}");
    }
}

#[test]
fn missing_subcommand_is_usage_error() {
    let env = TestEnv::new();
    let out = env.run(&[]);
    assert_code(&out, 2);
}

#[test]
fn unknown_subcommand_is_usage_error() {
    let env = TestEnv::new();
    let out = env.run(&["nope"]);
    assert_code(&out, 2);
    assert!(!stderr(&out).is_empty());
}
