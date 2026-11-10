//! `setup.json` 直接通过 database 的 FFI 执行（不经过 cyctron HTTP）。
//!
//! 依赖同级仓库 `sys/designplugin_lib` 的 database 构建产物；不存在时跳过。

#![cfg(windows)]

mod common;

use common::TestEnv;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

fn source_dir() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sys/designplugin_lib/.dytes");
    if dir.join("libary/database.dll").is_file() {
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
fn executes_setup_json_through_database_ffi() {
    let Some(source) = source_dir() else {
        eprintln!("跳过：未找到 sys/designplugin_lib 的 database 构建产物");
        return;
    };

    let env = TestEnv::new();
    let app = env.root().join("app");
    copy_dir(&source.join("libary"), &app.join("libary"));
    copy_dir(&source.join("database"), &app.join("database"));

    let setup = app.join("setup.json");
    let file = json!({
        "version": 1,
        "requests": [
            {
                "url": "http://127.0.0.1:44944/database",
                "headers": { "FFI-Symbol": "db_update" },
                "body": {
                    "path": "./plugins",
                    "sql": "CREATE TABLE IF NOT EXISTS ffi_test (id INTEGER PRIMARY KEY, name TEXT)"
                }
            },
            {
                "url": "http://127.0.0.1:44944/database",
                "headers": { "FFI-Symbol": "db_update" },
                "body": {
                    "path": "./plugins",
                    "sql": "DROP TABLE IF EXISTS ffi_test"
                },
                "allow_error": true
            }
        ]
    });
    fs::write(&setup, serde_json::to_string_pretty(&file).unwrap()).expect("写入 setup.json");

    let requests = dytes::update::script::load(setup.to_str().unwrap()).expect("读取 setup.json");
    assert_eq!(requests.len(), 2);

    dytes::update::script::execute(&app, &requests).expect("通过 database FFI 执行 setup.json");
}
