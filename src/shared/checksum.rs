//! 文件校验：SHA-256 全量摘要，大文件取首尾片段。
//!
//! 上传时生成 `FileChecksum` 写入远端 `dytes.json`；下载后按同一规则重算。
//! 大文件不读取全部内容，避免发布 / 更新时占用过多内存和时间。

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// 小于等于该大小时计算完整文件；超过时只计算首尾片段。
pub const FULL_HASH_LIMIT: u64 = 8 * 1024 * 1024;

/// 大文件首尾各取多少字节参与摘要。
pub const SAMPLE_CHUNK_BYTES: u64 = 4 * 1024 * 1024;

/// 单个文件的摘要信息，序列化到远端 `dytes.json` 的 `checksums`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileChecksum {
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub sampled: bool,
    #[serde(default)]
    pub sample_bytes: u64,
}

pub fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 计算文件摘要。大文件摘要输入为：文件大小（u64 LE）+ 首 4 MiB + 尾 4 MiB。
pub fn digest_file(path: &Path) -> Result<FileChecksum> {
    let size = fs::metadata(path)
        .with_context(|| format!("读取文件大小失败: {}", path.display()))?
        .len();
    let mut file = File::open(path).with_context(|| format!("打开文件失败: {}", path.display()))?;

    let sampled = size > FULL_HASH_LIMIT;
    let sample_bytes = if sampled { SAMPLE_CHUNK_BYTES } else { 0 };
    let mut hasher = Sha256::new();

    if sampled {
        hasher.update(size.to_le_bytes());
        let chunk = SAMPLE_CHUNK_BYTES as usize;
        let mut head = vec![0u8; chunk];
        file.read_exact(&mut head)
            .with_context(|| format!("读取文件头部失败: {}", path.display()))?;
        hasher.update(&head);

        file.seek(SeekFrom::Start(size - SAMPLE_CHUNK_BYTES))
            .with_context(|| format!("定位文件尾部失败: {}", path.display()))?;
        let mut tail = vec![0u8; chunk];
        file.read_exact(&mut tail)
            .with_context(|| format!("读取文件尾部失败: {}", path.display()))?;
        hasher.update(&tail);
    } else {
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .with_context(|| format!("读取文件失败: {}", path.display()))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
    }

    Ok(FileChecksum {
        sha256: hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        size,
        sampled,
        sample_bytes,
    })
}

/// 校验本地文件；大小不一致直接失败，摘要按 `digest_file` 的同一规则计算。
pub fn verify_file(path: &Path, expected: &FileChecksum) -> Result<()> {
    let actual = digest_file(path)?;
    if actual.size != expected.size {
        bail!(
            "文件大小不匹配: {}（期望 {}，实际 {}）",
            path.display(),
            expected.size,
            actual.size
        );
    }
    if actual.sampled != expected.sampled || actual.sample_bytes != expected.sample_bytes {
        bail!(
            "校验模式不匹配: {}（期望 sampled={} sample_bytes={}，实际 sampled={} sample_bytes={}）",
            path.display(),
            expected.sampled,
            expected.sample_bytes,
            actual.sampled,
            actual.sample_bytes
        );
    }
    if actual.sha256 != expected.sha256 {
        bail!(
            "SHA-256 不匹配: {}（期望 {}，实际 {}）",
            path.display(),
            expected.sha256,
            actual.sha256
        );
    }
    Ok(())
}

/// 按当前平台查找 checksum：优先 `<platform>/<key>`，回退旧清单的 `<key>`。
///
/// Linux / macOS 可能发布同名产物（例如 `cyctron`），新清单使用平台前缀
/// 区分；旧清单没有前缀时仍然兼容。
fn lookup_checksum<'a>(
    key: &str,
    checksums: &'a std::collections::HashMap<String, FileChecksum>,
) -> Option<&'a FileChecksum> {
    let platform = crate::shared::platform::current_platform();
    checksums
        .get(&format!("{}/{}", platform, key))
        .or_else(|| checksums.get(key))
}

/// 若远端清单包含该文件的 checksum，则校验；旧包没有 checksums 时保持兼容。
pub fn verify_if_present(
    path: &Path,
    key: &str,
    checksums: &std::collections::HashMap<String, FileChecksum>,
) -> Result<()> {
    if let Some(expected) = lookup_checksum(key, checksums) {
        verify_file(path, expected)?;
        println!("  校验通过: {} ({})", key, expected.sha256);
    }
    Ok(())
}

/// 下载后的校验入口：不匹配时删除损坏文件，避免后续继续使用。
pub fn verify_download(
    path: &Path,
    key: &str,
    checksums: &std::collections::HashMap<String, FileChecksum>,
) -> Result<()> {
    if let Some(expected) = lookup_checksum(key, checksums) {
        if let Err(error) = verify_file(path, expected) {
            let _ = fs::remove_file(path);
            return Err(error.context(format!("校验失败，已删除文件: {}", path.display())));
        }
        println!("  校验通过: {} ({})", key, expected.sha256);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn verifies_small_file_and_detects_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        fs::write(&path, b"hello checksum").unwrap();
        let checksum = digest_file(&path).unwrap();
        assert!(!checksum.sampled);
        verify_file(&path, &checksum).unwrap();

        fs::write(&path, b"hello checksum!").unwrap();
        assert!(verify_file(&path, &checksum).is_err());
    }

    #[test]
    fn samples_large_file_head_and_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.bin");
        let size = FULL_HASH_LIMIT + SAMPLE_CHUNK_BYTES;
        let mut file = File::create(&path).unwrap();
        file.set_len(size).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&vec![b'A'; SAMPLE_CHUNK_BYTES as usize])
            .unwrap();
        file.seek(SeekFrom::Start(size - SAMPLE_CHUNK_BYTES))
            .unwrap();
        file.write_all(&vec![b'Z'; SAMPLE_CHUNK_BYTES as usize])
            .unwrap();
        drop(file);

        let checksum = digest_file(&path).unwrap();
        assert!(checksum.sampled);
        assert_eq!(checksum.size, size);
        verify_file(&path, &checksum).unwrap();

        let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(size - 1)).unwrap();
        file.write_all(b"X").unwrap();
        drop(file);
        assert!(verify_file(&path, &checksum).is_err());
    }
}
