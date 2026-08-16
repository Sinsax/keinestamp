//! PSD 内嵌缩略图提取（方案 A）。
//!
//! 不解析完整 PSD 图层，只扫描 Image Resources 段中的缩略图资源
//! （Resource ID 1033 / 1036），取出内嵌 JPEG 作为预览图。
//! 这样不引入第三方 crate，也不需要外部转换工具；PSD 没有内嵌缩略图时返回 None。

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// 从 PSD 中提取内嵌 JPEG 缩略图，写入 `output_dir/{output_name}.jpg`。
/// 找不到可用缩略图时返回 `Ok(None)`（调用方降级为无预览）。
pub fn extract_thumbnail(
    psd_path: &Path,
    output_dir: &Path,
    output_name: &str,
) -> Result<Option<PathBuf>> {
    let bytes =
        std::fs::read(psd_path).with_context(|| format!("failed to read PSD {:?}", psd_path))?;

    if bytes.len() < 26 || &bytes[0..4] != b"8BPS" {
        return Ok(None);
    }

    let mut pos = 26usize;

    // Color mode data section: u32 length + data
    let Some(color_len) = read_u32(&bytes, pos) else {
        return Ok(None);
    };
    pos += 4;
    let Some(color_end) = pos.checked_add(color_len as usize) else {
        return Ok(None);
    };
    if color_end > bytes.len() {
        return Ok(None);
    }
    pos = color_end;

    // Image resources section: u32 length + resource blocks
    let Some(res_len) = read_u32(&bytes, pos) else {
        return Ok(None);
    };
    pos += 4;
    let Some(res_end) = pos.checked_add(res_len as usize) else {
        return Ok(None);
    };
    if res_end > bytes.len() {
        return Ok(None);
    }

    while pos + 12 <= res_end {
        if &bytes[pos..pos + 4] != b"8BIM" {
            // 资源段损坏或遇到未知结构：不再继续猜测，直接放弃提取。
            break;
        }
        let id = match read_u16(&bytes, pos + 4) {
            Some(v) => v,
            None => return Ok(None),
        };
        let name_len = bytes[pos + 6] as usize;
        let mut p = pos + 7 + name_len;
        // Pascal 字符串字段（含长度字节）需对齐到偶数
        if (name_len + 1) % 2 == 1 {
            p += 1;
        }
        let Some(data_len) = read_u32(&bytes, p) else {
            return Ok(None);
        };
        p += 4;
        let Some(data_end) = p.checked_add(data_len as usize) else {
            return Ok(None);
        };
        if data_end > bytes.len() {
            return Ok(None);
        }

        if id == 1033 || id == 1036 {
            if let Some(jpeg) = parse_thumbnail_data(&bytes[p..data_end]) {
                std::fs::create_dir_all(output_dir)?;
                let out_path = output_dir.join(format!("{}.jpg", output_name));
                std::fs::write(&out_path, jpeg)
                    .with_context(|| format!("failed to write PSD thumbnail {:?}", out_path))?;
                return Ok(Some(out_path));
            }
        }

        // 数据区按偶数对齐
        pos = if data_len % 2 == 1 {
            data_end + 1
        } else {
            data_end
        };
    }

    Ok(None)
}

/// 解析 1033/1036 缩略图资源数据，返回 JPEG 字节。
///
/// 数据结构（前 28 字节）：
/// - 0: format（1 = kJpegRGB）
/// - 4: width
/// - 8: height
/// - 12: widthbytes
/// - 16: total size
/// - 20: compressed size
/// - 24: bits per pixel
/// - 26: planes
/// - 28: JPEG 数据
fn parse_thumbnail_data(data: &[u8]) -> Option<&[u8]> {
    if data.len() < 28 {
        return None;
    }
    let format = u32::from_be_bytes(data[0..4].try_into().ok()?);
    if format != 1 {
        return None;
    }
    let compressed_size = u32::from_be_bytes(data[20..24].try_into().ok()?) as usize;
    if compressed_size == 0 {
        return None;
    }
    let start = 28usize;
    let end = start.checked_add(compressed_size)?;
    if end > data.len() {
        return None;
    }
    let jpeg = &data[start..end];
    // 简单校验 JPEG 头
    if jpeg.len() < 2 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return None;
    }
    Some(jpeg)
}

fn read_u32(bytes: &[u8], pos: usize) -> Option<u32> {
    let slice = bytes.get(pos..pos + 4)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u16(bytes: &[u8], pos: usize) -> Option<u16> {
    let slice = bytes.get(pos..pos + 2)?;
    Some(u16::from_be_bytes([slice[0], slice[1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("keinestamp_psd_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_jpeg() -> Vec<u8> {
        // 极简 JPEG 头 + 结尾；提取逻辑只校验 FF D8，不真正解码
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0xFF, 0xD9,
        ]
    }

    fn build_psd_with_thumbnail() -> Vec<u8> {
        let jpeg = fake_jpeg();
        let mut block = Vec::new();
        block.extend_from_slice(b"8BIM");
        block.extend_from_slice(&1036u16.to_be_bytes());
        block.push(0); // name length
        block.push(0); // pad to even string field
        // 28-byte thumbnail header
        let mut header = Vec::new();
        header.extend_from_slice(&1u32.to_be_bytes()); // format kJpegRGB
        header.extend_from_slice(&64u32.to_be_bytes());
        header.extend_from_slice(&48u32.to_be_bytes());
        header.extend_from_slice(&192u32.to_be_bytes());
        header.extend_from_slice(&(jpeg.len() as u32 + 28).to_be_bytes());
        header.extend_from_slice(&(jpeg.len() as u32).to_be_bytes());
        header.extend_from_slice(&24u16.to_be_bytes());
        header.extend_from_slice(&1u16.to_be_bytes());
        let data_len = header.len() + jpeg.len();
        block.extend_from_slice(&(data_len as u32).to_be_bytes());
        block.extend_from_slice(&header);
        block.extend_from_slice(&jpeg);
        if data_len % 2 == 1 {
            block.push(0);
        }

        let mut psd = Vec::new();
        psd.extend_from_slice(b"8BPS");
        psd.extend_from_slice(&1u16.to_be_bytes());
        psd.extend_from_slice(&[0u8; 6]);
        psd.extend_from_slice(&3u16.to_be_bytes()); // channels
        psd.extend_from_slice(&1u32.to_be_bytes()); // height
        psd.extend_from_slice(&1u32.to_be_bytes()); // width
        psd.extend_from_slice(&8u16.to_be_bytes()); // depth
        psd.extend_from_slice(&3u16.to_be_bytes()); // color mode RGB
        psd.extend_from_slice(&0u32.to_be_bytes()); // color mode data length
        psd.extend_from_slice(&(block.len() as u32).to_be_bytes());
        psd.extend_from_slice(&block);
        psd
    }

    #[test]
    fn test_extract_thumbnail_from_synthetic_psd() -> Result<()> {
        let dir = tmp_dir("extract");
        let psd_path = dir.join("sample.psd");
        std::fs::write(&psd_path, build_psd_with_thumbnail())?;

        let out = extract_thumbnail(&psd_path, &dir.join("out"), "thumb")?
            .expect("should extract embedded JPEG thumbnail");
        assert_eq!(out.file_name().unwrap().to_string_lossy(), "thumb.jpg");
        let bytes = std::fs::read(&out)?;
        assert_eq!(&bytes[..2], &[0xFF, 0xD8]);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_extract_thumbnail_returns_none_for_non_psd() -> Result<()> {
        let dir = tmp_dir("nonpsd");
        let path = dir.join("fake.psd");
        std::fs::write(&path, b"not a psd")?;
        assert!(extract_thumbnail(&path, &dir.join("out"), "thumb")?.is_none());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
