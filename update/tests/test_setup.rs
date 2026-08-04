use anyhow::Result;
use rquickjs::{Context, Runtime, Function, Object};
use std::fs;
use std::path::Path;

#[test]
fn test_setup_js() -> Result<()> {
    let script_path = Path::new("./test_web/init").join("setup.js");
    let script_content = if script_path.exists() {
        fs::read_to_string(&script_path)?
    } else {
        r#"
const response = await fetch("http://127.0.0.1:44944/database", {
    method: "POST",
    headers: {
        "Content-Type": "application/libary",
        "FFI-Symbol": "db_create"
    },
    body: JSON.stringify({
        path: "./router",
        table: "routers",
        schema: `id INTEGER PRIMARY KEY,
             title TEXT,
             icon TEXT,
             hide INTEGER,
             path TEXT,
             child TEXT,
             url TEXT,
             show INTEGER,
             parent_id INTEGER,
             level INTEGER`
    })
});
response;
"#
        .to_string()
    };

    // 1. 将 await fetch( 替换为 rustFetch(
    let mut transformed = script_content.replace("await fetch(", "rustFetch(");

    // 2. 自动添加返回语句：提取 const 变量名，追加到脚本末尾
    // 匹配模式：const 变量名 = rustFetch(...
    if let Some(cap) = regex::Regex::new(r"(?m)^\s*const\s+(\w+)\s*=\s*rustFetch\s*\(")
        .unwrap()
        .captures(&transformed)
    {
        let var_name = &cap[1];
        // 如果末尾没有分号或换行，添加换行和变量名
        if !transformed.trim_end().ends_with(&format!("{};", var_name)) {
            transformed = format!("{}\n{};", transformed.trim_end(), var_name);
        }
    }

    println!("转换后脚本内容:\n{}", transformed);

    let rt = Runtime::new()?;
    let ctx = Context::full(&rt)?;

    ctx.with(|ctx| -> Result<(), anyhow::Error> {
        let rust_fetch = Function::new(ctx.clone(), |url: String, options: Object| -> String {
            let method = options
                .get::<_, String>("method")
                .unwrap_or_else(|_| "GET".to_string());
            let headers = options.get::<_, Object>("headers").ok();
            let body = options.get::<_, String>("body").ok();

            let client = reqwest::blocking::Client::builder()
                .no_proxy()
                .build()
                .unwrap();

            let method_parsed = match method.to_uppercase().as_str() {
                "GET" => reqwest::Method::GET,
                "POST" => reqwest::Method::POST,
                "PUT" => reqwest::Method::PUT,
                "DELETE" => reqwest::Method::DELETE,
                "PATCH" => reqwest::Method::PATCH,
                _ => reqwest::Method::GET,
            };

            let mut request = client.request(method_parsed, &url);

            if let Some(headers_obj) = headers {
                let keys: Vec<String> = headers_obj
                    .keys::<String>()
                    .filter_map(Result::ok)
                    .collect();
                for key in keys {
                    if let Ok(value) = headers_obj.get::<_, String>(&key) {
                        request = request.header(&key, value);
                    }
                }
            }

            if let Some(body_str) = body {
                request = request.body(body_str);
            }

            match request.send() {
                Ok(response) => match response.text() {
                    Ok(text) => text,
                    Err(e) => format!("读取响应失败: {}", e),
                },
                Err(e) => format!("请求失败: {}", e),
            }
        })?;
        ctx.globals().set("rustFetch", rust_fetch)?;

        match ctx.eval::<rquickjs::Value, _>(transformed.as_str()) {
            Ok(result) => {
                println!("JS 执行成功");
                if let Some(s) = result.as_string() {
                    if let Ok(s) = s.to_string() {
                        println!("返回值: {}", s);
                    }
                } else {
                    println!("返回值: {:?}", result);
                }
            }
            Err(e) => {
                eprintln!("JS 执行失败: {}", e);
            }
        }

        Ok(())
    })?;

    Ok(())
}