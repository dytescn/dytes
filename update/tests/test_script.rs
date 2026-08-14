use anyhow::Result;
use std::path::PathBuf;
use update::utils::script::{extract_http_requests, extract_http_requests_to_vec, HttpRequest};

fn get_test_file(filename: &str) -> Option<PathBuf> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("test_web");
    path.push(filename);
    if path.exists() { Some(path) } else { None }
}

#[test]
fn test_init_js() -> Result<()> {
    let path = get_test_file("init.js").expect("测试文件 init.js 不存在");
    let requests = extract_http_requests_to_vec(path.to_str().unwrap())?;
    assert!(!requests.is_empty(), "init.js 应至少提取到一个请求");
    // 可以检查 URL 等
    assert!(requests.iter().all(|r| r.url == "http://127.0.0.1:44944/database"));
    Ok(())
}

#[test]
fn test_chat_ts() -> Result<()> {
    let path = get_test_file("chat.ts").expect("测试文件 chat.ts 不存在");
    let requests = extract_http_requests_to_vec(path.to_str().unwrap())?;
    assert!(requests.len() >= 10, "chat.ts 应至少提取到 10 个请求");
    Ok(())
}

#[test]
fn test_project_ts() -> Result<()> {
    let path = get_test_file("project.ts").expect("测试文件 project.ts 不存在");
    let requests = extract_http_requests_to_vec(path.to_str().unwrap())?;
    assert!(requests.len() >= 5, "project.ts 应至少提取到 5 个请求");
    // 检查第一个请求的方法等
    assert_eq!(requests[0].method, "POST");
    Ok(())
}