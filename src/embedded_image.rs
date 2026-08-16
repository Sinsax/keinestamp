//! 从工程文件中尽力提取第一张内嵌图片（JPEG/PNG）。
//!
//! 用于 PRPROJ / AEP / DRP 等没有稳定公开解析器的工程文件。
//! 原理是直接扫描文件字节中的 JPEG/PNG 魔数，提取完整图片数据。
//! 属于“尽力而为”，找不到时返回 None。

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const PNG_SIG: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
const PNG_IEND: &[u8] = &[
    0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];
const JPEG_SIG: &[u8] = &[0xFF, 0xD8, 0xFF];
const JPEG_EOI: &[u8] = &[0xFF, 0xD9];

/// 扫描文件并提取第一张完整内嵌图片，写入 `output_dir/{output_name}.{ext}`。
pub fn extract_first_image(
    file_path: &Path,
    output_dir: &Path,
    output_name: &str,
) -> Result<Option<PathBuf>> {
    let bytes = std::fs::read(file_path)
        .with_context(|| format!("failed to read project file {:?}", file_path))?;

    // PNG
    if let Some((start, end)) = find_embedded_png(&bytes) {
        let data = &bytes[start..end];
        if data.len() > 8 {
            std::fs::create_dir_all(output_dir)?;
            let out = output_dir.join(format!("{}.png", output_name));
            std::fs::write(&out, data)
                .with_context(|| format!("failed to write embedded PNG {:?}", out))?;
            return Ok(Some(out));
        }
    }

    // JPEG
    if let Some((start, end)) = find_embedded_jpeg(&bytes) {
        let data = &bytes[start..end];
        if data.len() > 16 {
            std::fs::create_dir_all(output_dir)?;
            let out = output_dir.join(format!("{}.jpg", output_name));
            std::fs::write(&out, data)
                .with_context(|| format!("failed to write embedded JPEG {:?}", out))?;
            return Ok(Some(out));
        }
    }

    Ok(None)
}

fn find_embedded_png(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut start = 0;
    while start + PNG_SIG.len() <= bytes.len() {
        if &bytes[start..start + PNG_SIG.len()] == PNG_SIG {
            if let Some(end_rel) = find_subslice(&bytes[start..], PNG_IEND) {
                let end = start + end_rel + PNG_IEND.len();
                return Some((start, end));
            }
        }
        start += 1;
    }
    None
}

fn find_embedded_jpeg(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut start = 0;
    while start + JPEG_SIG.len() <= bytes.len() {
        if &bytes[start..start + JPEG_SIG.len()] == JPEG_SIG {
            if let Some(end_rel) = find_subslice(&bytes[start + JPEG_SIG.len()..], JPEG_EOI) {
                let end = start + JPEG_SIG.len() + end_rel + JPEG_EOI.len();
                return Some((start, end));
            }
        }
        start += 1;
    }
    None
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_embedded_png() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("keinestamp_emb_png_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut fake = Vec::new();
        fake.extend_from_slice(b"header ");
        fake.extend_from_slice(PNG_SIG);
        fake.extend_from_slice(b"fake png body");
        fake.extend_from_slice(PNG_IEND);
        fake.extend_from_slice(b"tail");
        let src = dir.join("proj.bin");
        std::fs::write(&src, &fake)?;

        let out = extract_first_image(&src, &dir.join("out"), "thumb")?
            .expect("should extract embedded png");
        assert_eq!(out.extension().unwrap().to_string_lossy(), "png");
        assert!(std::fs::read(&out)?.starts_with(PNG_SIG));

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_extract_embedded_jpeg() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("keinestamp_emb_jpg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut fake = Vec::new();
        fake.extend_from_slice(b"prefix ");
        fake.extend_from_slice(JPEG_SIG);
        fake.extend_from_slice(b"fake jpeg body");
        fake.extend_from_slice(JPEG_EOI);
        let src = dir.join("proj.bin");
        std::fs::write(&src, &fake)?;

        let out = extract_first_image(&src, &dir.join("out"), "thumb")?
            .expect("should extract embedded jpeg");
        assert_eq!(out.extension().unwrap().to_string_lossy(), "jpg");

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
