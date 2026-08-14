//! 预览 Provider 抽象：文件类型 → 预览类型（glb / image / video / none）。
//!
//! 阶段 1：仅 glb（Blender 导出 GLB，行为与之前 `.blend` 时代完全一致）；
//! 配置化机制就绪后，后续添加新类型只需：默认映射加扩展名 + 新增 Provider 实现。

use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Glb,
    Image,
    Video,
    None,
}

impl PreviewKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PreviewKind::Glb => "glb",
            PreviewKind::Image => "image",
            PreviewKind::Video => "video",
            PreviewKind::None => "",
        }
    }
}

/// 预览配置：扩展名（小写，含点）→ 预览类型。
/// 阶段 1 默认仅 `.blend → glb`（行为不变）；后续类型一行加入。
pub struct PreviewConfig {
    glb: Vec<&'static str>,
    image: Vec<&'static str>,
    video: Vec<&'static str>,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            glb: vec![".blend"],
            image: vec![],
            video: vec![],
        }
    }
}

impl PreviewConfig {
    /// 文件路径 → 预览类型（扩展名小写匹配；未知扩展名 → None）
    pub fn kind_of(&self, path: &str) -> PreviewKind {
        let lower = path.to_lowercase();
        let ext = Path::new(&lower)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        if self.glb.iter().any(|e| *e == ext) {
            return PreviewKind::Glb;
        }
        if self.image.iter().any(|e| *e == ext) {
            return PreviewKind::Image;
        }
        if self.video.iter().any(|e| *e == ext) {
            return PreviewKind::Video;
        }
        PreviewKind::None
    }
}

/// 预览 Provider：根据源文件生成预览产物（返回产物路径；失败/不适用 → Ok(None) 降级）。
pub trait PreviewProvider {
    fn kind(&self) -> PreviewKind;
    /// `file` 源文件；`output_dir` 产物目录；`output_name` 产物基名（不含扩展名）
    fn generate(&self, file: &Path, output_dir: &Path, output_name: &str) -> Result<Option<PathBuf>>;
}

/// GLB Provider：Blender 导出（含瘦身：减面/贴图限尺寸/形态键移除；版本标记缓存）。
pub struct GlbProvider {
    pub blender_path: Option<PathBuf>,
}

impl PreviewProvider for GlbProvider {
    fn kind(&self) -> PreviewKind {
        PreviewKind::Glb
    }

    fn generate(&self, file: &Path, output_dir: &Path, output_name: &str) -> Result<Option<PathBuf>> {
        match crate::blender_render::export_to_glb(file, output_dir, output_name, self.blender_path.as_deref()) {
            Ok(path) => Ok(Some(path)),
            Err(e) => {
                eprintln!("[debug] GlbProvider: 导出跳过（降级为无预览）: {e}");
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_of_blend_is_glb() {
        let cfg = PreviewConfig::default();
        assert_eq!(cfg.kind_of("modeling/char.blend"), PreviewKind::Glb);
        assert_eq!(cfg.kind_of("CHAR.BLEND"), PreviewKind::Glb, "大小写不敏感");
        assert_eq!(cfg.kind_of("modeling/char.fbx"), PreviewKind::None, "阶段 1 未配置");
        assert_eq!(cfg.kind_of("notes.txt"), PreviewKind::None);
        assert_eq!(cfg.kind_of("noext"), PreviewKind::None);
    }

    #[test]
    fn test_kind_str() {
        assert_eq!(PreviewKind::Glb.as_str(), "glb");
        assert_eq!(PreviewKind::None.as_str(), "");
    }
}
