//! 设计插件 MCP 清单校验：与官方字段对齐的新增规则（当前为警告级）。

use dytes::utils::plugin_manifest::{check_design_plugin, CheckReport};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn write(path: impl AsRef<Path>, content: &str) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn manifest() -> Value {
    json!({
        "name": "cdr_check_txt",
        "version": "0.2.0",
        "type": "design_plugins",
        "plugins_type": "plugins_coreldraw",
        "targets": ["coreldraw"],
        "use_ai": true,
        "mcp": "mcp.json",
        "skills": "skills.json",
        "exports": {
            "./target/release/cdr_check_txt.dll": "cdr_check_txt.dll",
            "./mcp/mcp.json": "mcp.json",
            "./mcp/text_list.json": "text_list.json",
            "./skills.json": "skills.json"
        }
    })
}

fn prepare(dir: &Path) {
    write(dir.join("target/release/cdr_check_txt.dll"), "dll");
    write(dir.join("mcp/mcp.json"), r#"{"tools":["text_list.json"]}"#);
    write(dir.join("mcp/text_list.json"), "{}");
    write(
        dir.join("skills.json"),
        r#"{"skills":[{"id":"export","name":"导出","description":"","content":"内容","category":"design"}]}"#,
    );
}

/// 单工具清单：关闭 AI skills，并补齐唯一的工具文件导出。
fn single_tool_manifest() -> Value {
    let mut value = manifest();
    value["use_ai"] = json!(false);
    value.as_object_mut().unwrap().remove("skills");
    value["exports"] = json!({
        "./target/release/cdr_check_txt.dll": "cdr_check_txt.dll",
        "./mcp/mcp.json": "mcp.json",
        "./mcp/text_list.json": "text_list.json"
    });
    value
}

fn warnings(report: &CheckReport) -> Vec<&str> {
    report
        .items
        .iter()
        .filter(|item| item.warning)
        .map(|item| item.label.as_str())
        .collect()
}

#[test]
fn warns_on_tool_name_rules_without_failing() {
    let dir = tempfile::tempdir().unwrap();
    let value = single_tool_manifest();
    prepare(dir.path());
    let long_name = "a".repeat(140);
    let mcp = json!({
        "tools": [
            { "name": "bad name", "inputSchema": { "type": "object", "additionalProperties": false } },
            { "name": "bad name", "inputSchema": { "type": "object", "additionalProperties": false } },
            { "name": long_name, "inputSchema": { "type": "object", "additionalProperties": false } }
        ]
    });
    write(dir.path().join("mcp/mcp.json"), &mcp.to_string());

    let report = check_design_plugin(dir.path(), &value, "windows");
    assert!(report.passed(), "{:#?}", report.items);
    let labels = warnings(&report);
    assert!(labels.iter().any(|label| label.contains("name 字符集合法")));
    assert!(labels.iter().any(|label| label.contains("name 唯一")));
    assert!(labels.iter().any(|label| label.contains("name 长度合法")));
}

#[test]
fn warns_on_schema_rules_without_failing() {
    let dir = tempfile::tempdir().unwrap();
    let value = single_tool_manifest();
    prepare(dir.path());
    write(
        dir.path().join("mcp/mcp.json"),
        r#"{"tools":[{"name":"inline_no_schema"},"text_list.json"]}"#,
    );
    write(
        dir.path().join("mcp/text_list.json"),
        r#"{"name":"text_list","inputSchema":{"$schema":"https://example.com/schema","type":"object","properties":{}}}"#,
    );

    let report = check_design_plugin(dir.path(), &value, "windows");
    assert!(report.passed(), "{:#?}", report.items);
    let labels = warnings(&report);
    assert!(labels.iter().any(|label| label.contains("inputSchema 存在")));
    assert!(labels.iter().any(|label| label.contains("方言受支持")));
    assert!(labels.iter().any(|label| label.contains("无参声明")));
}

#[test]
fn warns_on_undeclared_transport_library() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = single_tool_manifest();
    prepare(dir.path());
    write(
        dir.path().join("mcp/mcp.json"),
        r#"{"tools":[{"name":"demo","transport":{"library":"info_lib","library_uuid":"uuid-1"}}]}"#,
    );

    let report = check_design_plugin(dir.path(), &value, "windows");
    assert!(report.passed(), "{:#?}", report.items);
    assert!(
        warnings(&report)
            .iter()
            .any(|label| label.contains("依赖已声明: info_lib")),
        "{:#?}",
        report.items
    );

    value["dependencies-sys"] = json!({ "info_lib": "0.1.0" });
    let report = check_design_plugin(dir.path(), &value, "windows");
    assert!(
        !warnings(&report)
            .iter()
            .any(|label| label.contains("依赖已声明: info_lib")),
        "{:#?}",
        report.items
    );
}

#[test]
fn accepts_aligned_tool_without_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = single_tool_manifest();
    value["dependencies-sys"] = json!({ "info_lib": "0.1.0" });
    prepare(dir.path());
    write(
        dir.path().join("mcp/mcp.json"),
        r#"{"tools":["text_list.json"]}"#,
    );
    write(
        dir.path().join("mcp/text_list.json"),
        r#"{
            "name": "text_list",
            "title": "文本清单",
            "description": "列出当前文档的文本块",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            },
            "outputSchema": {
                "type": "array",
                "items": { "type": "string" }
            },
            "annotations": { "readOnlyHint": true, "destructiveHint": false },
            "transport": { "library": "info_lib", "library_uuid": "uuid-1" }
        }"#,
    );

    let report = check_design_plugin(dir.path(), &value, "windows");
    assert!(report.passed(), "{:#?}", report.items);
    assert_eq!(report.warning_count(), 0, "{:#?}", report.items);
}

#[test]
fn rejects_inline_schema_not_object() {
    let dir = tempfile::tempdir().unwrap();
    let value = single_tool_manifest();
    prepare(dir.path());
    write(
        dir.path().join("mcp/mcp.json"),
        r#"{"tools":[{"name":"demo","inputSchema":"oops"}]}"#,
    );

    let report = check_design_plugin(dir.path(), &value, "windows");
    assert!(!report.passed());
    let labels: Vec<&str> = report.failures().map(|item| item.label.as_str()).collect();
    assert!(
        labels
            .iter()
            .any(|label| label.contains("inputSchema 为对象")),
        "{labels:?}"
    );
}
