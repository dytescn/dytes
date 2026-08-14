use anyhow::{anyhow, Result};
use oxc::{
    allocator::Allocator,
    ast::ast::{
        Argument, BindingIdentifier, BindingPattern, CallExpression, Expression,
        VariableDeclarator,
    },
    ast_visit::{walk, Visit},
    parser::Parser,
    span::SourceType,
};
use std::collections::HashMap;
use std::fs;

// ---------- HTTP 请求结构体 ----------

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: String, // JSON 字符串（标准格式）
}

// ---------- 主提取函数：返回结构化请求列表 ----------

pub fn extract_http_requests_to_vec(script_path: &str) -> Result<Vec<HttpRequest>> {
    let script = fs::read_to_string(script_path)
        .map_err(|e| anyhow!("读取文件失败: {}", e))?;

    // 1. 使用 oxc 解析 AST
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(script_path).unwrap_or(SourceType::mjs());
    let ret = Parser::new(&allocator, &script, source_type).parse();
    if ret.panicked {
        return Err(anyhow!("解析脚本失败: {:?}", ret.diagnostics));
    }
    let program = ret.program;

    // 2. 收集常量字符串
    let mut constants = HashMap::new();
    let mut collector = ConstantCollector { constants: &mut constants };
    collector.visit_program(&program);

    // 3. 提取所有 fetch 调用
    let mut visitor = FetchVisitor {
        requests: Vec::new(),
        source: script.clone(),
        constants: &constants,
    };
    visitor.visit_program(&program);

    // 4. 构建 HttpRequest 列表（body 已是标准 JSON）
    let mut http_requests = Vec::new();
    for req in &visitor.requests {
        let headers = parse_headers(&req.headers);
        http_requests.push(HttpRequest {
            url: req.url.clone(),
            method: req.method.clone(),
            headers,
            body: req.body.clone(),
        });
    }

    Ok(http_requests)
}

// ---------- 打印请求（调试用） ----------

pub fn extract_http_requests(script_path: &str) -> Result<()> {
    let requests = extract_http_requests_to_vec(script_path)?;
    for req in &requests {
        println!("========== HTTP 请求 ==========");
        println!("URL    : {}", req.url);
        println!("Method : {}", req.method);
        println!("Headers: {:?}", req.headers);
        println!("Body   : {}", req.body);
        println!("================================");
    }
    Ok(())
}

// ---------- 发送请求函数 ----------

pub fn send_http_requests(requests: Vec<HttpRequest>) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| anyhow!("构建 HTTP 客户端失败: {}", e))?;

    for (index, req) in requests.iter().enumerate() {
        println!("--- 发送请求 #{} ---", index + 1);
        println!("Method : {}", req.method);
        println!("URL    : {}", req.url);
        println!("Headers: {:?}", req.headers);
        println!("Body   : {}", req.body);

        let method = req
            .method
            .parse()
            .map_err(|e| anyhow!("无效的 HTTP 方法: {}", e))?;

        let mut builder = client.request(method, &req.url);

        for (key, value) in &req.headers {
            builder = builder.header(key, value);
        }

        if !req.body.is_empty() {
            builder = builder.body(req.body.clone());
        }

        match builder.send() {
            Ok(response) => {
                let status = response.status();
                println!("响应状态: {}", status);
                let response_text = response
                    .text()
                    .unwrap_or_else(|_| "读取响应失败".to_string());
                println!("响应内容: {}", response_text);
            }
            Err(e) => {
                eprintln!("请求 #{} 发送失败: {}", index + 1, e);
                return Err(anyhow!("请求发送失败: {}", e));
            }
        }
    }

    Ok(())
}

// ---------- 常量收集器 ----------

struct ConstantCollector<'a> {
    constants: &'a mut HashMap<String, String>,
}

impl<'a> Visit<'a> for ConstantCollector<'a> {
    fn visit_variable_declarator(&mut self, declarator: &VariableDeclarator<'a>) {
        if let Some(init) = &declarator.init {
            if let Expression::StringLiteral(s) = init {
                if let Some(ident) = get_binding_identifier(&declarator.id) {
                    self.constants.insert(ident.name.to_string(), s.value.to_string());
                }
            }
        }
        walk::walk_variable_declarator(self, declarator);
    }
}

fn get_binding_identifier<'a, 'b>(
    pattern: &'b BindingPattern<'a>,
) -> Option<&'b BindingIdentifier<'a>> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some(ident),
        _ => None,
    }
}

// ---------- fetch 访问器 ----------

#[derive(Default)]
struct FetchRequest {
    url: String,
    method: String,
    headers: String,
    body: String, // JSON 字符串
}

struct FetchVisitor<'a> {
    requests: Vec<FetchRequest>,
    source: String,
    constants: &'a HashMap<String, String>,
}

impl<'a> Visit<'a> for FetchVisitor<'a> {
    fn visit_call_expression(&mut self, expr: &CallExpression<'a>) {
        if let Expression::Identifier(ident) = &expr.callee {
            if ident.name.as_str() == "fetch" {
                if let Some(req) = parse_fetch_call(expr, &self.source, self.constants) {
                    self.requests.push(req);
                }
            }
        }
        walk::walk_call_expression(self, expr);
    }
}

fn parse_fetch_call(
    expr: &CallExpression,
    source: &str,
    constants: &HashMap<String, String>,
) -> Option<FetchRequest> {
    let args = &expr.arguments;
    if args.len() < 2 {
        return None;
    }

    let url = match &args[0] {
        Argument::StringLiteral(s) => s.value.to_string(),
        Argument::Identifier(id) => constants
            .get(id.name.as_str())
            .cloned()
            .unwrap_or_else(|| id.name.to_string()),
        _ => "<复杂表达式>".to_string(),
    };

    let options_obj = match &args[1] {
        Argument::ObjectExpression(obj) => obj,
        _ => return Some(FetchRequest {
            url,
            method: "<无法解析>".to_string(),
            headers: "<无法解析>".to_string(),
            body: "<无法解析>".to_string(),
        }),
    };

    let span = options_obj.span;
    let options_text = &source[span.start as usize..span.end as usize];

    let method = extract_field_from_options_text(options_text, "method")
        .map(clean_quotes)
        .unwrap_or_else(|| "GET".to_string());
    let headers = extract_field_from_options_text(options_text, "headers")
        .unwrap_or_else(|| "(无)".to_string());
    let body_raw = extract_field_from_options_text(options_text, "body")
        .unwrap_or_else(|| "(无)".to_string());
    let body_clean = extract_json_stringify_inner(&body_raw);

    // 将对象字面量转换为标准 JSON 字符串（使用 serde_json 确保转义正确）
    let body = if body_clean.trim_start().starts_with('{') {
        parse_object_literal_to_json(&body_clean, constants)
    } else {
        // 如果不是对象，尝试作为字符串处理（通常 body 是 JSON.stringify 对象）
        body_clean
    };

    Some(FetchRequest { url, method, headers, body })
}

// ---------- 辅助解析函数 ----------

fn extract_field_from_options_text(options_text: &str, field_name: &str) -> Option<String> {
    let pattern = format!("{}:", field_name);
    let start = options_text.find(&pattern)?;
    let after_colon = &options_text[start + pattern.len()..];
    let value_start = after_colon.find(|c: char| !c.is_whitespace())?;
    let value_part = &after_colon[value_start..];

    let mut depth = 0;
    let mut end = 0;
    let bytes = value_part.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    end = i;
                    break;
                }
                depth -= 1;
            }
            b'\'' | b'"' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
            }
            b',' if depth == 0 => {
                end = i;
                break;
            }
            _ => {}
        }
        i += 1;
    }

    let value = if end > 0 { &value_part[..end] } else { value_part };
    Some(value.trim().to_string())
}

fn clean_quotes(s: String) -> String {
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        if (bytes[0] == b'"' && bytes[s.len()-1] == b'"') ||
           (bytes[0] == b'\'' && bytes[s.len()-1] == b'\'') {
            return s[1..s.len()-1].to_string();
        }
    }
    s
}

fn find_matching_paren(s: &str, open_pos: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0;
    let mut i = open_pos;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'\'' | b'"' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn extract_json_stringify_inner(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.starts_with("JSON.stringify(") {
        let open_pos = trimmed.find('(').unwrap();
        if let Some(close_pos) = find_matching_paren(trimmed, open_pos) {
            let inner = &trimmed[open_pos + 1..close_pos];
            return inner.trim().to_string();
        }
    }
    s.to_string()
}

/// 将对象字面量（如 `{ path: "./router", sql: "..." }`）转换为标准 JSON 字符串
fn parse_object_literal_to_json(obj_text: &str, constants: &HashMap<String, String>) -> String {
    let trimmed = obj_text.trim();
    let inner = if trimmed.starts_with('{') && trimmed.ends_with('}') {
        &trimmed[1..trimmed.len()-1]
    } else {
        trimmed
    };

    let parts = split_top_level_commas(inner);
    let mut json_map = serde_json::Map::new();

    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if let Some(colon_pos) = find_top_level_colon(part) {
            let key_raw = part[..colon_pos].trim();
            let value_raw = part[colon_pos+1..].trim();

            let key = clean_quotes(key_raw.to_string());
            let value = value_to_json(value_raw, constants);
            json_map.insert(key, value);
        } else {
            // 简写属性（如 `{ path }`）→ `"path": "path"`
            let key = clean_quotes(part.to_string());
            let value = constants
                .get(&key)
                .cloned()
                .map(serde_json::Value::String)
                .unwrap_or_else(|| serde_json::Value::String(key.clone()));
            json_map.insert(key, value);
        }
    }

    serde_json::Value::Object(json_map).to_string()
}

/// 将单个值表达式转换为 serde_json::Value
fn value_to_json(value: &str, constants: &HashMap<String, String>) -> serde_json::Value {
    let trimmed = value.trim();

    // 字符串字面量（双引号或单引号）
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        let quote = trimmed.as_bytes()[0] as char;
        if trimmed.len() >= 2 && trimmed.ends_with(quote) {
            let inner = &trimmed[1..trimmed.len()-1];
            let unescaped = unescape_js_string(inner);
            return serde_json::Value::String(unescaped);
        }
    }

    // 模板字符串（反引号）
    if trimmed.starts_with('`') && trimmed.ends_with('`') {
        let inner = &trimmed[1..trimmed.len()-1];
        let unescaped = unescape_js_string(inner);
        return serde_json::Value::String(unescaped);
    }

    // 数字
    if let Ok(num) = trimmed.parse::<i64>() {
        return serde_json::Value::Number(num.into());
    }
    if let Ok(num) = trimmed.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(num) {
            return serde_json::Value::Number(num);
        }
    }

    // 布尔和 null
    if trimmed == "true" {
        return serde_json::Value::Bool(true);
    }
    if trimmed == "false" {
        return serde_json::Value::Bool(false);
    }
    if trimmed == "null" {
        return serde_json::Value::Null;
    }

    // 嵌套对象（递归）
    if trimmed.starts_with('{') {
        return serde_json::from_str(&parse_object_literal_to_json(trimmed, constants))
            .unwrap_or(serde_json::Value::Null);
    }

    // 数组（暂不处理，原样作为字符串）
    if trimmed.starts_with('[') {
        return serde_json::Value::String(trimmed.to_string());
    }

    // 标识符：优先从常量表替换，否则作为字符串
    if let Some(val) = constants.get(trimmed) {
        return serde_json::Value::String(val.clone());
    }

    serde_json::Value::String(trimmed.to_string())
}

/// 简易 JS 字符串反转义（处理常见的 \n, \t, \\, \", \' 等）
fn unescape_js_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    'r' => result.push('\r'),
                    '\\' => result.push('\\'),
                    '"' => result.push('"'),
                    '\'' => result.push('\''),
                    '`' => result.push('`'),
                    '0' => result.push('\0'),
                    other => {
                        result.push('\\');
                        result.push(other);
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'\'' | b'"' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
            }
            b',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
}

fn find_top_level_colon(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'\'' | b'"' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
            }
            b':' if depth == 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

// ---------- 解析 Headers 字符串为 HashMap ----------

fn parse_headers(headers_str: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let trimmed = headers_str.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        let inner = &trimmed[1..trimmed.len()-1];
        for part in inner.split(',') {
            if let Some(colon_pos) = part.find(':') {
                let key = part[..colon_pos].trim().trim_matches(|c| c == '"' || c == '\'');
                let value = part[colon_pos+1..].trim().trim_matches(|c| c == '"' || c == '\'');
                map.insert(key.to_string(), value.to_string());
            }
        }
    } else {
        for part in trimmed.split(',') {
            if let Some(colon_pos) = part.find(':') {
                let key = part[..colon_pos].trim();
                let value = part[colon_pos+1..].trim();
                map.insert(key.to_string(), value.to_string());
            }
        }
    }
    map
}