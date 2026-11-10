use std::env;

fn main() {
    let vars = [
        ("UPYUN_API_KEY", "DYTES_BUILTIN_UPYUN_API_KEY"),
        ("UPYUN_API_SECRET", "DYTES_BUILTIN_UPYUN_API_SECRET"),
        ("UPYUN_BUCKET", "DYTES_BUILTIN_UPYUN_BUCKET"),
        ("UPYUN_ENDPOINT", "DYTES_BUILTIN_UPYUN_ENDPOINT"),
    ];

    for (source, target) in vars {
        println!("cargo:rerun-if-env-changed={source}");
        if let Ok(value) = env::var(source) {
            let value = value.trim();
            if !value.is_empty() {
                println!("cargo:rustc-env={target}={value}");
            }
        }
    }

    // 仅在启用 `gui` feature 时编译 Slint 界面。
    if env::var_os("CARGO_FEATURE_GUI").is_some() {
        println!("cargo:rerun-if-changed=ui/install.slint");
        slint_build::compile("ui/install.slint").expect("编译 ui/install.slint 失败");
        println!("cargo:rerun-if-changed=ui/update.slint");
        slint_build::compile("ui/update.slint").expect("编译 ui/update.slint 失败");
    }
}
