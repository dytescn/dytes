//! 通过 FFI 调用 `plugin_lib` 的安装 / 更新测试。
//!
//! 依赖同级仓库 `sys/designplugin_lib` 的构建产物；不存在时跳过。

#![cfg(windows)]

mod common;

use common::TestEnv;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

fn source_dir() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sys/designplugin_lib/.dytes");
    if dir.join("libary/plugin_lib.dll").is_file() {
        Some(dir)
    } else {
        None
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("创建目录");
    for entry in fs::read_dir(from).expect("读取目录") {
        let entry = entry.expect("目录项");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("复制文件");
        }
    }
}

#[test]
fn installs_and_updates_plugin_via_ffi() {
    let Some(source) = source_dir() else {
        eprintln!("跳过：未找到 sys/designplugin_lib 的构建产物");
        return;
    };

    let env = TestEnv::new();
    let app = env.root().join("app");
    copy_dir(&source.join("libary"), &app.join("libary"));
    copy_dir(&source.join("database"), &app.join("database"));

    let package = app.join("plugins/demo");
    fs::create_dir_all(&package).expect("创建插件目录");

    let uuid = "11111111-1111-4111-8111-111111111111";
    let manifest = json!({
        "name": "demo",
        "uuid": uuid,
        "version": "0.1.0",
        "type": "design_plugins",
        "plugins_type": "plugins",
        "targets": ["common"],
        "use_ai": false,
        "platform-exports": {
            "windows": { "./demo.dll": "demo.dll" }
        }
    });
    fs::write(
        package.join("dytes.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .expect("写入插件清单");
    fs::copy(
        source.join("libary/plugin_lib.dll"),
        package.join("demo.dll"),
    )
    .expect("复制插件库");

    let installed = dytes::utils::plugin_lib::install(&app, &package).expect("通过 FFI 安装插件");
    assert_eq!(installed["name"], "demo");
    assert_eq!(installed["version"], "0.1.0");
    assert_eq!(installed["skipped"], false);

    let updated = dytes::utils::plugin_lib::update(&app, &package).expect("通过 FFI 更新插件");
    assert_eq!(updated["plugin_uuid"], uuid);
    assert_eq!(updated["skipped"], true);
}
