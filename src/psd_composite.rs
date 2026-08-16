//! PSD 合并图像数据解码（兜底方案）。
//!
//! 有些 PSD（例如 Krita 导出）没有 Photoshop 内嵌缩略图资源，但通常会包含
//! 合并后的图像数据（Image Data section）。本模块直接解码该数据并输出 PNG。
//!
//! 当前支持：
//! - 8-bit RGB / 灰度
//! - Raw 未压缩
//! - RLE（PackBits）压缩
//! 不支持：16/32-bit、CMYK、ZIP 压缩等复杂情况（返回 None 降级）。

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

/// 从 PSD 中提取合并图像，写入 `output_dir/{output_name}.png`。
/// 不支持/解析失败时返回 `Ok(None)`。
pub fn extract_composite(
    psd_path: &Path,
    output_dir: &Path,
    output_name: &str,
) -> Result<Option<PathBuf>> {
    let bytes =
        std::fs::read(psd_path).with_context(|| format!("failed to read PSD {:?}", psd_path))?;

    if bytes.len() < 26 || &bytes[0..4] != b"8BPS" {
        return Ok(None);
    }

    let Some(channels) = read_u16(&bytes, 12).map(|v| v as usize) else {
        return Ok(None);
    };
    let Some(height) = read_u32(&bytes, 14).map(|v| v as usize) else {
        return Ok(None);
    };
    let Some(width) = read_u32(&bytes, 18).map(|v| v as usize) else {
        return Ok(None);
    };
    let Some(depth) = read_u16(&bytes, 22).map(|v| v as usize) else {
        return Ok(None);
    };
    let Some(color_mode) = read_u16(&bytes, 24).map(|v| v as usize) else {
        return Ok(None);
    };

    if depth != 8 || width == 0 || height == 0 {
        return Ok(None);
    }

    let mut pos = 26usize;

    // Color mode data
    let Some(color_len) = read_u32(&bytes, pos).map(|v| v as usize) else {
        return Ok(None);
    };
    pos += 4;
    let Some(next) = pos.checked_add(color_len) else {
        return Ok(None);
    };
    pos = next;
    if pos > bytes.len() {
        return Ok(None);
    }

    // Image resources
    let Some(res_len) = read_u32(&bytes, pos).map(|v| v as usize) else {
        return Ok(None);
    };
    pos += 4;
    let Some(next) = pos.checked_add(res_len) else {
        return Ok(None);
    };
    pos = next;
    if pos > bytes.len() {
        return Ok(None);
    }

    // Layer and mask info
    let Some(layer_len) = read_u32(&bytes, pos).map(|v| v as usize) else {
        return Ok(None);
    };
    pos += 4;
    let Some(next) = pos.checked_add(layer_len) else {
        return Ok(None);
    };
    pos = next;
    if pos > bytes.len() {
        return Ok(None);
    }

    // Image data section
    let row_bytes = width; // 8-bit
    let mut channel_data: Vec<Vec<u8>> = Vec::new();
    for _ in 0..channels {
        let Some(compression) = read_u16(&bytes, pos) else {
            return Ok(None);
        };
        pos += 2;
        match compression {
            0 => {
                let Some(len) = height.checked_mul(row_bytes) else {
                    return Ok(None);
                };
                let Some(end) = pos.checked_add(len) else {
                    return Ok(None);
                };
                if end > bytes.len() {
                    return Ok(None);
                }
                channel_data.push(bytes[pos..end].to_vec());
                pos = end;
            }
            1 => {
                let mut data = Vec::with_capacity(height * row_bytes);
                let mut row_lengths = Vec::with_capacity(height);
                for _ in 0..height {
                    let Some(l) = read_u16(&bytes, pos).map(|v| v as usize) else {
                        return Ok(None);
                    };
                    pos += 2;
                    row_lengths.push(l);
                }
                for l in row_lengths {
                    let Some(end) = pos.checked_add(l) else {
                        return Ok(None);
                    };
                    if end > bytes.len() {
                        return Ok(None);
                    }
                    let Some(row) = decode_rle(&bytes[pos..end], row_bytes) else {
                        return Ok(None);
                    };
                    data.extend_from_slice(&row);
                    pos = end;
                }
                channel_data.push(data);
            }
            _ => return Ok(None),
        }
    }

    let rgb = match color_mode {
        3 => {
            // RGB：取前三个通道 R,G,B
            if channel_data.len() < 3 {
                return Ok(None);
            }
            let r = &channel_data[0];
            let g = &channel_data[1];
            let b = &channel_data[2];
            if r.len() < width * height || g.len() < width * height || b.len() < width * height {
                return Ok(None);
            }
            let mut out = Vec::with_capacity(width * height * 3);
            for i in 0..width * height {
                out.push(r[i]);
                out.push(g[i]);
                out.push(b[i]);
            }
            out
        }
        1 => {
            // 灰度：单通道复制为 RGB
            if channel_data.is_empty() || channel_data[0].len() < width * height {
                return Ok(None);
            }
            let gray = &channel_data[0];
            let mut out = Vec::with_capacity(width * height * 3);
            for i in 0..width * height {
                out.push(gray[i]);
                out.push(gray[i]);
                out.push(gray[i]);
            }
            out
        }
        _ => return Ok(None),
    };

    std::fs::create_dir_all(output_dir)?;
    let out_path = output_dir.join(format!("{}.png", output_name));
    write_png(&out_path, width, height, &rgb)
        .with_context(|| format!("failed to write PSD composite PNG {:?}", out_path))?;
    Ok(Some(out_path))
}

/// PSD RLE（PackBits）解码一行。
fn decode_rle(data: &[u8], expected: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while i < data.len() && out.len() < expected {
        let header = data[i] as i8;
        i += 1;
        if header >= 0 {
            let count = header as usize + 1;
            if i + count > data.len() {
                return None;
            }
            out.extend_from_slice(&data[i..i + count]);
            i += count;
        } else if header != -128 {
            let count = (1 - header as i32) as usize; // 2..=128
            if i >= data.len() {
                return None;
            }
            let byte = data[i];
            i += 1;
            for _ in 0..count {
                out.push(byte);
            }
        }
    }
    if out.len() == expected {
        Some(out)
    } else {
        None
    }
}

fn read_u16(bytes: &[u8], pos: usize) -> Option<u16> {
    let s = bytes.get(pos..pos + 2)?;
    Some(u16::from_be_bytes([s[0], s[1]]))
}

fn read_u32(bytes: &[u8], pos: usize) -> Option<u32> {
    let s = bytes.get(pos..pos + 4)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn write_png(path: &Path, width: usize, height: usize, rgb: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;

    // PNG signature
    file.write_all(&[137, 80, 78, 71, 13, 10, 26, 10])?;

    // IHDR
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(height as u32).to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(2); // color type RGB
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    file.write_all(&png_chunk(b"IHDR", &ihdr))?;

    // Raw image data with filter byte 0 per row
    let stride = width * 3;
    let mut raw = Vec::with_capacity((stride + 1) * height);
    for y in 0..height {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * stride..(y + 1) * stride]);
    }

    // IDAT
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raw)?;
    let compressed = encoder.finish()?;
    file.write_all(&png_chunk(b"IDAT", &compressed))?;

    // IEND
    file.write_all(&png_chunk(b"IEND", &[]))?;
    Ok(())
}

fn png_chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let len = (data.len() as u32).to_be_bytes();
    let mut type_and_data = Vec::with_capacity(4 + data.len());
    type_and_data.extend_from_slice(tag);
    type_and_data.extend_from_slice(data);
    let crc = crc32(&type_and_data).to_be_bytes();
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&len);
    out.extend_from_slice(&type_and_data);
    out.extend_from_slice(&crc);
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xffffffff;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb88320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "keinestamp_psd_comp_{}_{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn build_raw_rgb_psd(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
        let channels = 3u16;
        let mut psd = Vec::new();
        psd.extend_from_slice(b"8BPS");
        psd.extend_from_slice(&1u16.to_be_bytes());
        psd.extend_from_slice(&[0u8; 6]);
        psd.extend_from_slice(&channels.to_be_bytes());
        psd.extend_from_slice(&height.to_be_bytes());
        psd.extend_from_slice(&width.to_be_bytes());
        psd.extend_from_slice(&8u16.to_be_bytes());
        psd.extend_from_slice(&3u16.to_be_bytes()); // RGB
        psd.extend_from_slice(&0u32.to_be_bytes()); // color mode data
        psd.extend_from_slice(&0u32.to_be_bytes()); // image resources
        psd.extend_from_slice(&0u32.to_be_bytes()); // layer/mask

        let plane_len = (width * height) as usize;
        for c in 0..3 {
            psd.extend_from_slice(&0u16.to_be_bytes()); // raw
            for i in 0..plane_len {
                psd.push(rgb[i * 3 + c]);
            }
        }
        psd
    }

    #[test]
    fn test_extract_composite_raw_rgb() -> Result<()> {
        let dir = tmp_dir("raw");
        let w = 2;
        let h = 2;
        let rgb = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let psd = build_raw_rgb_psd(w, h, &rgb);
        let psd_path = dir.join("test.psd");
        std::fs::write(&psd_path, psd)?;

        let out = extract_composite(&psd_path, &dir.join("out"), "thumb")?
            .expect("should extract composite");
        assert_eq!(out.file_name().unwrap().to_string_lossy(), "thumb.png");
        let bytes = std::fs::read(&out)?;
        assert_eq!(&bytes[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_extract_composite_returns_none_for_non_psd() -> Result<()> {
        let dir = tmp_dir("nonpsd");
        let path = dir.join("fake.psd");
        std::fs::write(&path, b"not psd")?;
        assert!(extract_composite(&path, &dir.join("out"), "thumb")?.is_none());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
