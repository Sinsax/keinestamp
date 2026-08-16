//! Krita 原生 `.kra` 预览提取。
//!
//! `.kra` 本质是 ZIP 容器，内部通常包含 `preview.png` 或 `mergedimage.png`。
//! 这里直接读取压缩包内的 PNG 作为预览图，不需要外部工具。

use anyhow::{Context, Result};
use std::io::Read;
use std::path::{Path, PathBuf};

/// 从 `.kra` 中提取预览 PNG，写入 `output_dir/{output_name}.png`。
/// 找不到可用预览时返回 `Ok(None)`。
pub fn extract_preview(
    kra_path: &Path,
    output_dir: &Path,
    output_name: &str,
) -> Result<Option<PathBuf>> {
    let file = std::fs::File::open(kra_path)
        .with_context(|| format!("failed to open KRA {:?}", kra_path))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to read KRA as zip {:?}", kra_path))?;

    // Krita 常见预览文件名：先找到目标索引，避免在循环中持有 ZipFile 借用
    let candidates = ["preview.png", "mergedimage.png"];
    let mut chosen_index: Option<usize> = None;
    for i in 0..archive.len() {
        let Ok(zf) = archive.by_index(i) else {
            continue;
        };
        let name = zf.name().to_lowercase();
        if candidates
            .iter()
            .any(|c| name == *c || name.ends_with(&format!("/{}", c)))
        {
            chosen_index = Some(i);
            break;
        }
    }

    let Some(idx) = chosen_index else {
        return Ok(None);
    };

    let mut zf = archive
        .by_index(idx)
        .with_context(|| format!("failed to open preview in KRA {:?}", kra_path))?;
    let mut bytes = Vec::new();
    zf.read_to_end(&mut bytes)
        .with_context(|| format!("failed to read preview from KRA {:?}", kra_path))?;

    if bytes.is_empty() {
        return Ok(None);
    }

    std::fs::create_dir_all(output_dir)?;
    let out_path = output_dir.join(format!("{}.png", output_name));
    std::fs::write(&out_path, &bytes)
        .with_context(|| format!("failed to write KRA preview {:?}", out_path))?;
    Ok(Some(out_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_extract_preview_from_synthetic_kra() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("keinestamp_kra_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let kra_path = dir.join("test.kra");
        let file = std::fs::File::create(&kra_path)?;
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("preview.png", zip::write::SimpleFileOptions::default())?;
        zip.write_all(b"fake png preview")?;
        zip.finish()?;

        let out = extract_preview(&kra_path, &dir.join("out"), "thumb")?
            .expect("should extract KRA preview");
        assert_eq!(out.file_name().unwrap().to_string_lossy(), "thumb.png");
        assert_eq!(std::fs::read(&out)?, b"fake png preview");

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_extract_preview_returns_none_for_missing() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("keinestamp_kra_none_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let kra_path = dir.join("empty.kra");
        let file = std::fs::File::create(&kra_path)?;
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("document.xml", zip::write::SimpleFileOptions::default())?;
        zip.write_all(b"<kra/>")?;
        zip.finish()?;

        assert!(extract_preview(&kra_path, &dir.join("out"), "thumb")?.is_none());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
