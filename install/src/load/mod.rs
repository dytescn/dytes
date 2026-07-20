pub mod load;
pub mod linux;
pub mod mac;
pub mod win;
pub mod types;

// use load::xxx
pub use linux::LINUX_DOWNLOAD_LIST;
pub use load::download_file_with_progress;
pub use mac::MACOS_DOWNLOAD_LIST;
pub use win::WIN_DOWNLOAD_LIST;
