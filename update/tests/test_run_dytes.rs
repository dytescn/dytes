// // tests/test_run_dytes.rs
// use update::run::dytes::{init_dytes, load_config};
// use std::fs;
// use serde_json::json;

// #[tokio::test]
// async fn test_init_dytes_success() {
//     // 创建测试配置文件
//     let test_config = json!({
//         "name": "test_app",
//         "version": "1.0.0"
//     });
//     fs::write("dytes.json", serde_json::to_string_pretty(&test_config).unwrap()).unwrap();
    
//     // 测试初始化
//     let result = init_dytes().await;
//     assert!(result.is_ok());
    
//     // 清理
//     fs::remove_file("dytes.json").ok();
// }

// #[test]
// fn test_load_config_success() {
//     // 创建测试配置文件
//     let test_config = json!({
//         "name": "test_app",
//         "version": "2.0.0"
//     });
//     fs::write("dytes.json", serde_json::to_string_pretty(&test_config).unwrap()).unwrap();
    
//     let result = load_config();
//     assert!(result.is_ok());
    
//     let (name, version) = result.unwrap();
//     assert_eq!(name, "test_app");
//     assert_eq!(version, "2.0.0");
    
//     // 清理
//     fs::remove_file("dytes.json").unwrap();
// }

// #[test]
// fn test_load_config_file_not_found() {
//     // 确保配置文件不存在
//     fs::remove_file("dytes.json").ok();
    
//     let result = load_config();
//     assert!(result.is_err());
//     // 检查错误信息是否包含文件名
//     let err_msg = result.unwrap_err().to_string();
//     assert!(err_msg.contains("dytes.json") || err_msg.contains("不存在"));
// }

// #[test]
// fn test_load_config_default_values() {
//     // 创建只有 name 没有 version 的配置文件
//     let test_config = json!({
//         "name": "test_app"
//     });
//     fs::write("dytes.json", serde_json::to_string_pretty(&test_config).unwrap()).unwrap();
    
//     let result = load_config();
//     assert!(result.is_ok());
    
//     let (name, version) = result.unwrap();
//     assert_eq!(name, "test_app");
//     assert_eq!(version, "1.0.0"); // 使用默认值
    
//     // 清理
//     fs::remove_file("dytes.json").unwrap();
// }

// #[test]
// fn test_load_config_missing_name() {
//     // 创建只有 version 没有 name 的配置文件
//     let test_config = json!({
//         "version": "3.0.0"
//     });
//     fs::write("dytes.json", serde_json::to_string_pretty(&test_config).unwrap()).unwrap();
    
//     let result = load_config();
//     assert!(result.is_ok());
    
//     let (name, version) = result.unwrap();
//     assert_eq!(name, "piksel"); // 使用默认值
//     assert_eq!(version, "3.0.0");
    
//     // 清理
//     fs::remove_file("dytes.json").unwrap();
// }

// #[test]
// fn test_load_config_invalid_json() {
//     // 创建无效的 JSON
//     fs::write("dytes.json", "invalid json content").unwrap();
    
//     let result = load_config();
//     assert!(result.is_err());
    
//     // 清理
//     fs::remove_file("dytes.json").ok();
// }

// #[test]
// fn test_load_config_empty_file() {
//     // 创建空文件
//     fs::write("dytes.json", "").unwrap();
    
//     let result = load_config();
//     assert!(result.is_err());
    
//     // 清理
//     fs::remove_file("dytes.json").ok();
// }