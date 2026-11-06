use anyhow::{Context as AnyhowContext, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::update::utils::script::{
    HttpRequest, extract_http_requests_to_vec, send_http_requests,
};

const PID_FILE: &str = "./run.pid";

/// 检测 cyctron.exe 是否运行（若 PID 文件存在但进程已死，则清理文件并返回 false）
pub fn is_cyctron_running() -> bool {
    let pid_path = Path::new(PID_FILE);
    if !pid_path.exists() {
        return false;
    }

    let content = match fs::read_to_string(PID_FILE) {
        Ok(c) => c,
        Err(_) => {
            let _ = fs::remove_file(PID_FILE);
            return false;
        }
    };

    let pid: u32 = match content.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            let _ = fs::remove_file(PID_FILE);
            return false;
        }
    };

    let alive = is_process_alive(pid);
    if !alive {
        let _ = fs::remove_file(PID_FILE);
    }
    alive
}

/// 检测进程是否存活
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, 0) == 0
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let _ = CloseHandle(handle);
                true
            }
            Err(_) => false,
        }
    }
}

/// 停止 cyctron.exe（公共导出）
pub fn stop_cyctron() -> Result<()> {
    if !is_cyctron_running() {
        println!("  cyctron.exe 未运行");
        let _ = fs::remove_file(PID_FILE);
        return Ok(());
    }

    let content = fs::read_to_string(PID_FILE)?;
    let pid: u32 = content.trim().parse()?;

    #[cfg(windows)]
    {
        let output = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output()?;

        if output.status.success() {
            println!("  cyctron.exe (PID: {}) 已停止", pid);
            let _ = fs::remove_file(PID_FILE);
            Ok(())
        } else {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("停止 cyctron.exe 失败: {}", err_msg))
        }
    }

    #[cfg(unix)]
    {
        unsafe {
            if libc::kill(pid as i32, libc::SIGTERM) == 0 {
                println!("  cyctron.exe (PID: {}) 已停止", pid);
                let _ = fs::remove_file(PID_FILE);
                Ok(())
            } else {
                Err(anyhow::anyhow!("停止 cyctron.exe 失败"))
            }
        }
    }
}

/// 等待本机端口就绪（cyctron 启动后 setup 请求需要服务已监听）
fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let address = format!("127.0.0.1:{port}");
    let deadline = std::time::Instant::now() + timeout;

    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect_timeout(
            &address.parse().expect("invalid address"),
            Duration::from_millis(500),
        )
        .is_ok()
        {
            return true;
        }
        thread::sleep(Duration::from_millis(200));
    }

    false
}

/// 启动 cyctron.exe（公共导出）
pub fn start_cyctron() -> Result<()> {
    let exe_name = if cfg!(windows) {
        "cyctron.exe"
    } else {
        "cyctron"
    };
    if is_cyctron_running() {
        println!("  {exe_name} 已在运行");
        return Ok(());
    }

    let exe = if cfg!(windows) {
        "./cyctron.exe"
    } else {
        "./cyctron"
    };
    println!("  启动: {}", exe);

    // 兼容旧安装：0.1.14 之前下载的 core 产物可能是 0644。
    #[cfg(unix)]
    if Path::new(exe).exists() {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(exe) {
            let mut permissions = metadata.permissions();
            if permissions.mode() & 0o111 == 0 {
                permissions.set_mode(0o755);
                let _ = fs::set_permissions(exe, permissions);
                println!("  已设置执行位: {}", exe);
            }
        }
    }

    let child = Command::new(exe)
        .arg("start")
        .spawn()
        .with_context(|| format!("启动 {exe_name} 失败"))?;

    thread::sleep(Duration::from_millis(500));
    if wait_for_port(44944, Duration::from_secs(30)) {
        println!("  cyctron 端口 44944 已就绪");
    } else {
        println!("  警告: 等待 cyctron 端口 44944 超时");
    }

    #[cfg(windows)]
    let pid = child.id();

    #[cfg(unix)]
    let pid = child.id();

    fs::write(PID_FILE, pid.to_string())?;
    println!("  {exe_name} (PID: {}) 已启动", pid);

    Ok(())
}

/// 优先用本机 JS/TS 运行时真正执行 init 脚本。
///
/// 静态提取无法求值函数形参、循环和条件；有 Deno / Bun / Node 时直接执行，
/// 才能覆盖 `setup.ts` 里封装函数、`for...of` 和 try/catch 的场景。
fn run_init_with_runtime(script_path: &str) -> Option<Result<()>> {
    let path = Path::new(script_path);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file = path.file_name()?.to_string_lossy().to_string();

    let candidates: [(&str, &[&str]); 3] = [
        (
            "deno",
            &["run", "--allow-net", "--allow-read", "--allow-env", "--allow-write"],
        ),
        ("bun", &["run"]),
        ("node", &["--experimental-strip-types"]),
    ];

    for (program, args) in candidates {
        match Command::new(program)
            .args(args)
            .arg(&file)
            .current_dir(parent)
            .output()
        {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.trim().is_empty() {
                    println!("{}", stdout.trim());
                }
                println!("  init 脚本执行成功（{program}）");
                return Some(Ok(()));
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Some(Err(anyhow::anyhow!(
                    "init 脚本执行失败（{program}）: {}",
                    stderr.trim()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Some(Err(anyhow::anyhow!(
                    "启动 {program} 执行 init 脚本失败: {error}"
                )));
            }
        }
    }

    None
}

/// 执行单个 init 脚本（解析脚本中的 HTTP 请求并发送）
async fn execute_init_script(script_path: &str) -> Result<()> {
    println!("  执行初始化脚本: {}", script_path);

    // 有运行时优先真正执行；静态提取只作为无运行时环境的回退。
    if let Some(result) = run_init_with_runtime(script_path) {
        return result;
    }

    // 1. 使用 oxc 静态解析脚本，提取所有 HTTP 请求
    let requests = extract_http_requests_to_vec(script_path)?;
    if requests.is_empty() {
        anyhow::bail!(
            "init 脚本未包含可执行的 HTTP 请求（且未找到 deno / bun / node）: {}",
            script_path
        );
    }

    // 2. 打印提取到的所有请求（便于调试）
    println!("  提取到 {} 个 HTTP 请求", requests.len());
    for (i, req) in requests.iter().enumerate() {
        println!("  [请求 {}]", i + 1);
        println!("    Method : {}", req.method);
        println!("    URL    : {}", req.url);
        println!("    Headers: {:?}", req.headers);
        println!("    Body   : {}", req.body);
    }

    // 3. 按顺序发送请求
    send_http_requests(requests).await?;

    println!("  init 脚本执行成功");
    Ok(())
}

/// 插件安装后触发 `plugin_lib` 扫描：登记 plugins.db、导入 kv 集合。
///
/// 由 `dytes install plugin` 调用；`plugin_lib` 自己调用 dytes 时设置
/// `DYTES_SKIP_PLUGIN_SCAN=1`，避免在 cyctron 请求处理中再次回调自身。
pub async fn scan_plugin_registry() -> Result<()> {
    if std::env::var_os("DYTES_SKIP_PLUGIN_SCAN").is_some() {
        return Ok(());
    }

    let was_running = is_cyctron_running();
    if !was_running {
        start_cyctron()?;
    }

    let request = HttpRequest {
        url: "http://127.0.0.1:44944/plugin_lib".to_string(),
        method: "POST".to_string(),
        headers: HashMap::from([
            (
                "Content-Type".to_string(),
                "application/libary".to_string(),
            ),
            ("FFI-Symbol".to_string(), "post_scan".to_string()),
        ]),
        body: serde_json::json!({ "dir": "./plugins", "persist": true }).to_string(),
    };

    let result = send_http_requests(vec![request]).await;

    if !was_running {
        if let Err(error) = stop_cyctron() {
            eprintln!("  关闭 cyctron 失败: {}", error);
        }
    }

    result
}

/// 执行单个包内的所有 init 脚本（不管理 cyctron 生命周期，需外部保证 cyctron 已启动）
pub async fn setup_webs_init(web_dir: &str, init_scripts: &[String]) -> Result<()> {
    if init_scripts.is_empty() {
        println!("  没有 init 脚本需要执行");
        return Ok(());
    }

    let mut has_error = false;
    for script in init_scripts {
        let clean_script = script.trim_start_matches("./");
        let script_path = Path::new(web_dir).join(clean_script);

        if script_path.exists() {
            if let Err(e) = execute_init_script(script_path.to_str().unwrap_or(script)).await {
                eprintln!("  执行 init 脚本 {} 失败: {}", script, e);
                has_error = true;
            }
        } else {
            eprintln!("  init 脚本不存在: {}", script_path.display());
            has_error = true;
        }
    }

    if has_error {
        Err(anyhow::anyhow!("部分脚本执行失败"))
    } else {
        Ok(())
    }
}

/// 批量执行所有 Web 包的 init 脚本（统一管理 cyctron 生命周期）
pub async fn run_all_init_scripts(scripts_list: Vec<(String, Vec<String>)>) -> Result<()> {
    if scripts_list.is_empty() {
        println!("  没有需要执行的 init 脚本");
        return Ok(());
    }

    println!(
        "  检测到 {} 个 Web 包需要执行 init 脚本，统一处理...",
        scripts_list.len()
    );

    // 1. 启动 cyctron
    start_cyctron()?;

    // 2. 依次执行每个包的所有脚本
    let mut overall_error = false;
    for (web_dir, init_scripts) in scripts_list {
        println!("  执行包: {}", web_dir);
        if let Err(e) = setup_webs_init(&web_dir, &init_scripts).await {
            eprintln!("  包 {} 的脚本执行失败: {}", web_dir, e);
            overall_error = true;
        }
    }

    // 3. 统一关闭 cyctron（并删除 PID 文件）
    println!("  关闭 cyctron.exe...");
    if let Err(e) = stop_cyctron() {
        eprintln!("  关闭 cyctron.exe 失败: {}", e);
        let _ = fs::remove_file(PID_FILE);
    }

    if overall_error {
        Err(anyhow::anyhow!("部分包的 init 脚本执行失败"))
    } else {
        println!("  所有 init 脚本执行完成");
        Ok(())
    }
}
