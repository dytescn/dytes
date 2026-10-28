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
}
