use anyhow::{anyhow, Result};
use rquickjs::{Context, Ctx, Function, Object, Runtime, Value};
use rquickjs::function::Func;
use std::fs;

/// 独立的 fetch 实现函数，生命周期标注明确
fn fetch_impl<'js>(ctx: Ctx<'js>, url: String, options: Value<'js>) -> rquickjs::Result<Value<'js>> {
    // 解析 options 对象
    let opts = options
        .as_object()
        .ok_or_else(|| rquickjs::Error::new_from_js("fetch", "options 必须是对象"))?;

    // 获取请求方法（默认 GET）
    let method = opts
        .get::<_, String>("method")
        .unwrap_or_else(|_| "GET".to_string());

    println!("========== HTTP 请求 ==========");
    println!("URL    : {}", url);
    println!("Method : {}", method);

    // 打印 headers
    if let Ok(headers_obj) = opts.get::<_, Object>("headers") {
        println!("Headers:");
        for prop in headers_obj.props::<Value, Value>() {
            if let Ok((key, value)) = prop {
                if let (Some(key_str), Some(value_str)) = (key.as_string(), value.as_string()) {
                    let key = key_str.to_string().unwrap_or_else(|_| "<无效>".to_string());
                    let value = value_str.to_string().unwrap_or_else(|_| "<无效>".to_string());
                    println!("  {}: {}", key, value);
                }
            }
        }
    } else {
        println!("Headers: (无)");
    }

    // 打印 body
    if let Ok(body_val) = opts.get::<_, Value>("body") {
        if let Some(body_str) = body_val.as_string() {
            let body = body_str.to_string().unwrap_or_else(|_| "<无效>".to_string());
            println!("Body   : {}", body);
        } else {
            println!("Body   : (非字符串类型)");
        }
    } else {
        println!("Body   : (无)");
    }
    println!("================================");

    // 构造模拟 Response 对象
    let resp = Object::new(ctx.clone())?;
    resp.set("ok", true)?;
    resp.set("status", 200)?;

    // 构造 json() 方法，返回空对象 {}
    let empty_json = ctx.json_parse("{}")?;
    let json_fn = Function::new(ctx.clone(), move |_ctx: Ctx| -> rquickjs::Result<Value> {
        Ok(empty_json.clone())
    })?;
    resp.set("json", json_fn)?;

    Ok(resp.into_value())
}

pub fn execute_init_script(script_path: &str) -> Result<()> {
    println!("执行初始化脚本: {}", script_path);

    // 1. 读取脚本内容
    let script = fs::read_to_string(script_path)
        .map_err(|e| anyhow!("读取文件失败: {}", e))?;

    // 2. 创建 JS 运行时和上下文
    let runtime = Runtime::new()
        .map_err(|e| anyhow!("创建 JS 运行时失败: {}", e))?;
    let context = Context::full(&runtime)
        .map_err(|e| anyhow!("创建 JS 上下文失败: {}", e))?;

    // 3. 注入 fetch 并执行脚本
    context.with(|ctx| {
        // 使用 Func::from 包装独立函数，避免生命周期问题
        let fetch_func = Func::from(fetch_impl);
        ctx.globals().set("fetch", fetch_func)?;

        // 包裹脚本以支持顶层 await
        let wrapped_script = format!("(async () => {{\n{}\n}})();", script);

        // 执行脚本
        match ctx.eval::<Value<'_>, _>(wrapped_script.as_str()) {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("JS 脚本执行失败: {}", e)),
        }
    })
    .map_err(|e| anyhow!("执行 JS 脚本失败: {}", e))?;

    Ok(())
}