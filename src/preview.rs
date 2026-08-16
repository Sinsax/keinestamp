//! 预览 Provider 抽象：文件类型 → 预览类型（glb / image / video / none）。
//!
//! 当前实现：
//! - Glb：Blender 导出 GLB（原 `.blend` 行为）
//! - Image：常见图片缩略图 + PSD 内嵌缩略图提取
//! - Video：ffmpeg 取视频首帧
//!
//! 配置存放于项目私有目录 `.keinestamp/preview.toml`（不参与 Lore 提交）。
//! 文件缺失时使用内置默认映射；显式配置某一类型时，该类型使用配置的完整列表。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
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

    /// 根据**预览产物**路径推断类型（timeline.json 旧数据没有 preview_type 时的兜底）。
    /// 这里看的是产物扩展名，不是源文件扩展名；源文件映射由 `PreviewConfig::kind_of` 负责。
    pub fn from_preview_path(path: &str) -> Self {
        let lower = path.to_lowercase();
        if lower.ends_with(".glb") {
            return PreviewKind::Glb;
        }
        const IMAGE_EXTS: &[&str] = &[
            ".png", ".jpg", ".jpeg", ".webp", ".tga", ".exr", ".psd", ".kra", ".sai", ".clip",
        ];
        if IMAGE_EXTS.iter().any(|e| lower.ends_with(e)) {
            return PreviewKind::Image;
        }
        const VIDEO_EXTS: &[&str] = &[".mp4", ".mov", ".webm", ".prproj", ".aep", ".drp"];
        if VIDEO_EXTS.iter().any(|e| lower.ends_with(e)) {
            return PreviewKind::Video;
        }
        PreviewKind::None
    }
}

/// 预览配置：扩展名（小写，含点）→ 预览类型。
/// 默认包含 glb / image / video 三类；用户可在 `.keinestamp/preview.toml` 覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewConfig {
    #[serde(default = "default_glb")]
    pub glb: Vec<String>,
    #[serde(default = "default_image")]
    pub image: Vec<String>,
    #[serde(default = "default_video")]
    pub video: Vec<String>,
}

fn default_glb() -> Vec<String> {
    vec![".blend".to_string(), ".glb".to_string()]
}

fn default_image() -> Vec<String> {
    vec![
        ".png".to_string(),
        ".jpg".to_string(),
        ".jpeg".to_string(),
        ".webp".to_string(),
        ".tga".to_string(),
        ".exr".to_string(),
        ".psd".to_string(),
        ".kra".to_string(),
        ".sai".to_string(),
        ".clip".to_string(),
    ]
}

fn default_video() -> Vec<String> {
    vec![
        ".mp4".to_string(),
        ".mov".to_string(),
        ".webm".to_string(),
        ".prproj".to_string(),
        ".aep".to_string(),
        ".drp".to_string(),
    ]
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            glb: default_glb(),
            image: default_image(),
            video: default_video(),
        }
    }
}

impl PreviewConfig {
    /// 项目私有预览配置路径：`.keinestamp/preview.toml`
    pub fn preview_config_path(project_root: &Path) -> PathBuf {
        project_root.join(".keinestamp").join("preview.toml")
    }

    /// 读取项目预览配置；文件不存在时返回默认配置。
    /// 解析失败返回错误，由调用方决定是阻断还是降级。
    pub fn load(project_root: &Path) -> Result<Self> {
        let path = Self::preview_config_path(project_root);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read preview config {:?}", path))?;
        let cfg: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse preview config {:?}", path))?;
        Ok(cfg.normalized())
    }

    /// 读取项目预览配置；失败时降级为默认配置并打印调试信息，不阻塞主流程。
    pub fn load_or_default(project_root: &Path) -> Self {
        match Self::load(project_root) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("[debug] PreviewConfig::load 失败，使用默认配置: {e}");
                Self::default()
            }
        }
    }

    /// 将当前配置写入项目私有目录（原子写）。
    pub fn save(&self, project_root: &Path) -> Result<()> {
        let path = Self::preview_config_path(project_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        crate::process_util::atomic_write(&path, content.as_bytes())
            .with_context(|| format!("failed to write preview config {:?}", path))
    }

    /// 文件路径 → 预览类型（扩展名小写匹配；未知扩展名 → None）
    pub fn kind_of(&self, path: &str) -> PreviewKind {
        let ext = Self::extension_of(path);
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

    /// 返回某个预览类型对应的 Provider；尚未实现的类型返回 None。
    pub fn provider_for(
        kind: PreviewKind,
        blender_path: Option<PathBuf>,
    ) -> Option<Box<dyn PreviewProvider>> {
        match kind {
            PreviewKind::Glb => Some(Box::new(GlbProvider { blender_path })),
            PreviewKind::Image => Some(Box::new(ImageProvider)),
            PreviewKind::Video => Some(Box::new(VideoProvider)),
            PreviewKind::None => None,
        }
    }

    fn extension_of(path: &str) -> String {
        let lower = path.to_lowercase();
        Path::new(&lower)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default()
    }

    /// 规范化：扩展名统一小写并补前导点、去重。
    fn normalized(mut self) -> Self {
        for list in [&mut self.glb, &mut self.image, &mut self.video] {
            let mut seen = std::collections::HashSet::new();
            list.retain(|e| {
                let normalized = normalize_ext(e);
                if normalized.is_empty() || seen.contains(&normalized) {
                    return false;
                }
                seen.insert(normalized.clone());
                true
            });
            *list = list.iter().map(|e| normalize_ext(e)).collect();
        }
        self
    }
}

fn normalize_ext(ext: &str) -> String {
    let ext = ext.trim().to_lowercase();
    if ext.is_empty() {
        return String::new();
    }
    if ext.starts_with('.') {
        ext
    } else {
        format!(".{}", ext)
    }
}

/// 预览 Provider：根据源文件生成预览产物（返回产物路径；失败/不适用 → Ok(None) 降级）。
pub trait PreviewProvider {
    fn kind(&self) -> PreviewKind;
    /// 产物子目录名（相对 web_root），例如 `previews_glb`。
    fn output_subdir(&self) -> &'static str;
    /// `file` 源文件；`output_dir` 产物目录；`output_name` 产物基名（不含扩展名）
    fn generate(
        &self,
        file: &Path,
        output_dir: &Path,
        output_name: &str,
    ) -> Result<Option<PathBuf>>;
}

/// GLB Provider：Blender 导出（含瘦身：减面/贴图限尺寸/形态键移除；版本标记缓存）。
pub struct GlbProvider {
    pub blender_path: Option<PathBuf>,
}

impl PreviewProvider for GlbProvider {
    fn kind(&self) -> PreviewKind {
        PreviewKind::Glb
    }

    fn output_subdir(&self) -> &'static str {
        "previews_glb"
    }

    fn generate(
        &self,
        file: &Path,
        output_dir: &Path,
        output_name: &str,
    ) -> Result<Option<PathBuf>> {
        // 已经是 GLB 的文件直接复制，无需 Blender 转换
        if file
            .extension()
            .map(|e| e.eq_ignore_ascii_case("glb"))
            .unwrap_or(false)
        {
            std::fs::create_dir_all(output_dir)?;
            let out = output_dir.join(format!("{}.glb", output_name));
            std::fs::copy(file, &out)?;
            return Ok(Some(out));
        }

        match crate::blender_render::export_to_glb(
            file,
            output_dir,
            output_name,
            self.blender_path.as_deref(),
        ) {
            Ok(path) => Ok(Some(path)),
            Err(e) => {
                eprintln!("[debug] GlbProvider: 导出跳过（降级为无预览）: {e}");
                Ok(None)
            }
        }
    }
}

/// 图片 Provider：生成图片缩略图。
///
/// - PSD：优先提取内嵌 JPEG 缩略图（方案 A），失败后尝试 ffmpeg，再失败降级无预览。
/// - 常见 web 图片：优先用 ffmpeg 转成 JPG 缩略图；ffmpeg 不可用时直接复制原图。
/// - TGA/EXR 等浏览器不直接支持的格式：需要 ffmpeg，否则无预览。
pub struct ImageProvider;

impl PreviewProvider for ImageProvider {
    fn kind(&self) -> PreviewKind {
        PreviewKind::Image
    }

    fn output_subdir(&self) -> &'static str {
        "previews_image"
    }

    fn generate(
        &self,
        file: &Path,
        output_dir: &Path,
        output_name: &str,
    ) -> Result<Option<PathBuf>> {
        std::fs::create_dir_all(output_dir)?;
        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // Web 可直接显示的格式：优先原样复制，保留透明通道和画质；缩略图优化后续再做
        if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
            let copied = output_dir.join(format!("{}.{}", output_name, ext));
            std::fs::copy(file, &copied)?;
            return Ok(Some(copied));
        }

        // PSD：先走纯 Rust 内嵌缩略图提取，再尝试合并图像数据解码
        if ext == "psd" {
            match crate::psd_thumbnail::extract_thumbnail(file, output_dir, output_name) {
                Ok(Some(path)) => return Ok(Some(path)),
                Ok(None) => eprintln!("[debug] ImageProvider: PSD 无内嵌缩略图，尝试合并图像解码"),
                Err(e) => eprintln!("[debug] ImageProvider: PSD 缩略图提取失败: {e}"),
            }
            match crate::psd_composite::extract_composite(file, output_dir, output_name) {
                Ok(Some(path)) => return Ok(Some(path)),
                Ok(None) => eprintln!("[debug] ImageProvider: PSD 合并图像解码不可用，尝试 ffmpeg"),
                Err(e) => eprintln!("[debug] ImageProvider: PSD 合并图像解码失败: {e}"),
            }
        }

        // KRA：Krita 原生格式，直接读取压缩包内预览 PNG
        if ext == "kra" {
            match crate::kra_thumbnail::extract_preview(file, output_dir, output_name) {
                Ok(Some(path)) => return Ok(Some(path)),
                Ok(None) => eprintln!("[debug] ImageProvider: KRA 无 preview.png/mergedimage.png"),
                Err(e) => eprintln!("[debug] ImageProvider: KRA 预览提取失败: {e}"),
            }
        }

        // TGA/EXR/PSD 等：用 ffmpeg 转 JPG（无 ffmpeg 时降级无预览）
        let jpg_out = output_dir.join(format!("{}.jpg", output_name));
        if run_ffmpeg_thumbnail(file, &jpg_out, false)? {
            return Ok(Some(jpg_out));
        }

        Ok(None)
    }
}

/// 视频 Provider：用 ffmpeg 取首帧生成封面图。
/// 工程文件（prproj/aep/drp）暂不自动取帧，返回无预览。
pub struct VideoProvider;

impl PreviewProvider for VideoProvider {
    fn kind(&self) -> PreviewKind {
        PreviewKind::Video
    }

    fn output_subdir(&self) -> &'static str {
        "previews_video"
    }

    fn generate(
        &self,
        file: &Path,
        output_dir: &Path,
        output_name: &str,
    ) -> Result<Option<PathBuf>> {
        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if matches!(ext.as_str(), "prproj" | "aep" | "drp") {
            // 工程文件没有稳定公开解析器：尽力扫描内嵌图片作为封面
            return crate::embedded_image::extract_first_image(file, output_dir, output_name);
        }

        std::fs::create_dir_all(output_dir)?;
        let out = output_dir.join(format!("{}.jpg", output_name));
        if run_ffmpeg_thumbnail(file, &out, true)? {
            Ok(Some(out))
        } else {
            Ok(None)
        }
    }
}

/// 返回可用的 ffmpeg 命令；未安装或不可执行时返回 None。
/// 支持 `KEINESTAMP_FFMPEG` 环境变量指定路径。
fn ffmpeg_cmd() -> Option<std::process::Command> {
    let program = std::env::var("KEINESTAMP_FFMPEG")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "ffmpeg".to_string());

    let mut probe = std::process::Command::new(&program);
    probe.arg("-version");
    if probe.output().is_err() {
        eprintln!("[debug] ffmpeg 不可用（{}）", program);
        return None;
    }
    Some(crate::process_util::hide_console(
        std::process::Command::new(program),
    ))
}

/// 用 ffmpeg 生成缩略图/首帧。成功且产物存在返回 true，否则返回 false（不抛错）。
fn run_ffmpeg_thumbnail(input: &Path, output: &Path, seek_first_frame: bool) -> Result<bool> {
    let Some(mut cmd) = ffmpeg_cmd() else {
        return Ok(false);
    };
    if seek_first_frame {
        cmd.arg("-ss").arg("00:00:00.500");
    }
    cmd.arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-vf")
        .arg("scale='min(1280,iw)':'min(1280,ih)':force_original_aspect_ratio=decrease")
        .arg("-frames:v")
        .arg("1")
        .arg(output);

    let (status, _, stderr) = crate::process_util::run_with_timeout(
        &mut cmd,
        std::time::Duration::from_secs(30),
        "ffmpeg thumbnail",
    )?;
    if status.success() && output.exists() {
        Ok(true)
    } else {
        eprintln!(
            "[debug] ffmpeg thumbnail 失败: {}",
            String::from_utf8_lossy(&stderr)
        );
        Ok(false)
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
        assert_eq!(
            cfg.kind_of("modeling/char.glb"),
            PreviewKind::Glb,
            "GLB 文件可直接作为预览"
        );
        assert_eq!(
            cfg.kind_of("modeling/char.fbx"),
            PreviewKind::None,
            "阶段 1 未配置"
        );
        assert_eq!(cfg.kind_of("notes.txt"), PreviewKind::None);
        assert_eq!(cfg.kind_of("noext"), PreviewKind::None);
    }

    #[test]
    fn test_default_maps_image_and_video() {
        let cfg = PreviewConfig::default();
        assert_eq!(cfg.kind_of("textures/diffuse.png"), PreviewKind::Image);
        assert_eq!(cfg.kind_of("design.psd"), PreviewKind::Image);
        assert_eq!(cfg.kind_of("video.mp4"), PreviewKind::Video);
        assert_eq!(cfg.kind_of("project.prproj"), PreviewKind::Video);
        assert_eq!(cfg.kind_of("notes.txt"), PreviewKind::None);
    }

    #[test]
    fn test_provider_for_returns_implemented_providers() {
        assert!(PreviewConfig::provider_for(PreviewKind::Glb, None).is_some());
        assert!(PreviewConfig::provider_for(PreviewKind::Image, None).is_some());
        assert!(PreviewConfig::provider_for(PreviewKind::Video, None).is_some());
        assert!(PreviewConfig::provider_for(PreviewKind::None, None).is_none());
    }

    #[test]
    fn test_kind_str() {
        assert_eq!(PreviewKind::Glb.as_str(), "glb");
        assert_eq!(PreviewKind::None.as_str(), "");
    }

    #[test]
    fn test_from_preview_path() {
        assert_eq!(
            PreviewKind::from_preview_path("previews_glb/a.glb"),
            PreviewKind::Glb
        );
        assert_eq!(
            PreviewKind::from_preview_path("previews_image/a.PNG"),
            PreviewKind::Image
        );
        assert_eq!(
            PreviewKind::from_preview_path("previews_video/a.mp4"),
            PreviewKind::Video
        );
        assert_eq!(
            PreviewKind::from_preview_path("previews/a.bin"),
            PreviewKind::None
        );
    }

    #[test]
    fn test_load_missing_returns_default() {
        let dir =
            std::env::temp_dir().join(format!("keinestamp_preview_missing_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = PreviewConfig::load(&dir).unwrap();
        assert_eq!(cfg, PreviewConfig::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_save_and_load_roundtrip() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "keinestamp_preview_roundtrip_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let cfg = PreviewConfig {
            glb: vec![".blend".to_string(), ".fbx".to_string()],
            image: vec![".png".to_string(), "jpg".to_string()],
            video: vec![".mp4".to_string()],
        };
        cfg.save(&dir)?;

        let loaded = PreviewConfig::load(&dir)?;
        assert_eq!(loaded, cfg.normalized());
        assert_eq!(loaded.kind_of("a.jpg"), PreviewKind::Image);
        assert_eq!(loaded.kind_of("a.FBX"), PreviewKind::Glb);
        assert_eq!(loaded.kind_of("a.txt"), PreviewKind::None);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_load_normalizes_and_dedups() -> Result<()> {
        let dir =
            std::env::temp_dir().join(format!("keinestamp_preview_norm_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir.join(".keinestamp")).unwrap();
        std::fs::write(
            PreviewConfig::preview_config_path(&dir),
            "glb = [\".blend\", \"BLEND\", \".FBX\"]\nimage = [\"png\", \".PNG\"]\n",
        )?;

        let cfg = PreviewConfig::load(&dir)?;
        assert_eq!(cfg.glb, vec![".blend".to_string(), ".fbx".to_string()]);
        assert_eq!(cfg.image, vec![".png".to_string()]);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_glb_provider_copies_existing_glb() -> Result<()> {
        let dir =
            std::env::temp_dir().join(format!("keinestamp_preview_glbcopy_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let src = dir.join("model.glb");
        let fake_glb = b"fake glb content";
        std::fs::write(&src, fake_glb)?;

        let provider = GlbProvider { blender_path: None };
        let out = provider
            .generate(&src, &dir.join("out"), "thumb")?
            .expect("GLB 文件应直接复制生成预览");
        assert_eq!(std::fs::read(&out)?, fake_glb);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_image_provider_copies_web_friendly_image() -> Result<()> {
        let dir =
            std::env::temp_dir().join(format!("keinestamp_preview_imgcopy_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let src = dir.join("design.png");
        let fake_png = b"fake png bytes for copy test";
        std::fs::write(&src, fake_png)?;

        let provider = ImageProvider;
        let out = provider
            .generate(&src, &dir.join("out"), "thumb")?
            .expect("web 友好图片应直接复制生成预览");
        assert_eq!(std::fs::read(&out)?, fake_png);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_load_corrupt_returns_error() {
        let dir =
            std::env::temp_dir().join(format!("keinestamp_preview_corrupt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir.join(".keinestamp")).unwrap();
        std::fs::write(PreviewConfig::preview_config_path(&dir), "not toml").unwrap();

        assert!(PreviewConfig::load(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
