//! 统一多语言（i18n）。
//!
//! catalog 在仓库根 `i18n/{zh-CN,en-US}.json`，与前端 `frontend/src/lib/i18n` 共用同一份文件，
//! 不复制、不生成。键命名 `surface.area.item`，加载时展平成点号路径；`{name}` 为占位符。
//!
//! 取值优先级（见 `docs/superpowers/specs/2026-11-02-i18n-design.md`）：
//! `KEINESTAMP_LOCALE` env → `AppConfig.locale` → 系统语言 → `zh-CN`。

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::Serialize;

/// 当前支持的界面语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locale {
    ZhCn,
    EnUs,
}

impl Locale {
    pub fn tag(self) -> &'static str {
        match self {
            Locale::ZhCn => "zh-CN",
            Locale::EnUs => "en-US",
        }
    }

    /// 语言自身名称（在任意界面语言下都用母语展示，便于用户找回自己认识的语言）
    pub fn native_name(self) -> &'static str {
        match self {
            Locale::ZhCn => "简体中文",
            Locale::EnUs => "English",
        }
    }

    pub fn all() -> [Locale; 2] {
        [Locale::ZhCn, Locale::EnUs]
    }

    /// 宽松解析："zh" / "zh-CN" / "zh_CN.UTF-8" / "zh-Hans-CN" → ZhCn；"en*" → EnUs
    pub fn parse(raw: &str) -> Option<Locale> {
        let tag = normalize_tag(raw);
        if tag.is_empty() {
            return None;
        }
        let primary = tag.split('-').next().unwrap_or("");
        match primary {
            "zh" => Some(Locale::ZhCn),
            "en" => Some(Locale::EnUs),
            _ => None,
        }
    }
}

/// 用户的语言偏好（AppConfig.locale）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalePref {
    System,
    Fixed(Locale),
}

impl LocalePref {
    /// `None` / 空串 / "system" / "auto" → 跟随系统；无法识别的值也回落跟随系统。
    pub fn parse(raw: Option<&str>) -> LocalePref {
        let Some(value) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
            return LocalePref::System;
        };
        let tag = normalize_tag(value);
        if tag == "system" || tag == "auto" || tag == "follow" {
            return LocalePref::System;
        }
        Locale::parse(&tag).map(LocalePref::Fixed).unwrap_or(LocalePref::System)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LocalePref::System => "system",
            LocalePref::Fixed(l) => l.tag(),
        }
    }
}

const ZH_JSON: &str = include_str!("../i18n/zh-CN.json");
const EN_JSON: &str = include_str!("../i18n/en-US.json");

static CATALOG_ZH: OnceLock<HashMap<String, String>> = OnceLock::new();
static CATALOG_EN: OnceLock<HashMap<String, String>> = OnceLock::new();

const UNRESOLVED: u8 = 0;
const CACHED_ZH: u8 = 1;
const CACHED_EN: u8 = 2;
static CURRENT: AtomicU8 = AtomicU8::new(UNRESOLVED);

// ---------------------------------------------------------------- catalog

pub fn catalog(locale: Locale) -> &'static HashMap<String, String> {
    match locale {
        Locale::ZhCn => CATALOG_ZH.get_or_init(|| flatten_json(ZH_JSON)),
        Locale::EnUs => CATALOG_EN.get_or_init(|| flatten_json(EN_JSON)),
    }
}

fn flatten_json(src: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    match serde_json::from_str::<serde_json::Value>(src) {
        Ok(value) => flatten_value("", &value, &mut out),
        Err(e) => eprintln!("[i18n] catalog JSON 解析失败: {e}"),
    }
    out
}

fn flatten_value(prefix: &str, value: &serde_json::Value, out: &mut HashMap<String, String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_value(&path, child, out);
            }
        }
        serde_json::Value::String(text) => {
            out.insert(prefix.to_string(), text.clone());
        }
        other => {
            out.insert(prefix.to_string(), other.to_string());
        }
    }
}

// ---------------------------------------------------------------- locale

pub fn env_override() -> Option<String> {
    std::env::var("KEINESTAMP_LOCALE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 读取 AppConfig 里的偏好（每次从磁盘读；结果在 `locale()` 里缓存一次）。
pub fn pref() -> LocalePref {
    let raw = crate::app_config::AppConfig::load().locale;
    LocalePref::parse(raw.as_deref())
}

/// 纯函数版解析，便于单测（不碰缓存、不碰磁盘）。
pub fn resolve_with(env: Option<&str>, pref: LocalePref) -> Locale {
    if let Some(explicit) = env {
        if let Some(locale) = Locale::parse(explicit) {
            return locale;
        }
    }
    match pref {
        LocalePref::Fixed(locale) => locale,
        LocalePref::System => system_locale(),
    }
}

/// 进程内生效的界面语言（首次调用时解析并缓存）。
pub fn locale() -> Locale {
    match CURRENT.load(Ordering::Relaxed) {
        CACHED_ZH => Locale::ZhCn,
        CACHED_EN => Locale::EnUs,
        _ => {
            let resolved = resolve_with(env_override().as_deref(), pref());
            // 单元测试固定中文：多处断言检查中文错误文案，
            // 不能被开发机系统语言影响（集成测试另有 KEINESTAMP_LOCALE=zh-CN）。
            #[cfg(test)]
            let resolved = {
                let _ = resolved;
                Locale::ZhCn
            };
            set_locale(resolved);
            resolved
        }
    }
}

/// GUI 切换语言后同步进程内状态（后续 Rust 侧文案立即使用新语言）。
pub fn set_locale(locale: Locale) {
    CURRENT.store(
        match locale {
            Locale::ZhCn => CACHED_ZH,
            Locale::EnUs => CACHED_EN,
        },
        Ordering::Relaxed,
    );
}

/// 系统语言；探测不到或不属于已支持语言时回落 `zh-CN`。
pub fn system_locale() -> Locale {
    detect_system_tag()
        .as_deref()
        .and_then(Locale::parse)
        .unwrap_or(Locale::ZhCn)
}

fn detect_system_tag() -> Option<String> {
    #[cfg(windows)]
    {
        if let Some(tag) = windows_locale() {
            return Some(tag);
        }
    }
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// 直接调 kernel32，避免为一个函数引入第三方 crate。
#[cfg(windows)]
fn windows_locale() -> Option<String> {
    unsafe extern "system" {
        fn GetUserDefaultLocaleName(lp_locale_name: *mut u16, cch_locale_name: i32) -> i32;
    }
    // Windows 的 LOCALE_NAME_MAX_LENGTH
    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    let mut buf = [0u16; LOCALE_NAME_MAX_LENGTH];
    // SAFETY: buf 是可写的定长缓冲，长度与 cch 参数一致；返回 0 表示失败。
    let written = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), LOCALE_NAME_MAX_LENGTH as i32) };
    if written <= 0 {
        return None;
    }
    let len = (written as usize).saturating_sub(1).min(LOCALE_NAME_MAX_LENGTH - 1);
    String::from_utf16(&buf[..len]).ok()
}

fn normalize_tag(raw: &str) -> String {
    let trimmed = raw.trim();
    // "zh_CN.UTF-8" → "zh-cn"
    let head = trimmed.split('.').next().unwrap_or(trimmed);
    head.trim().replace('_', "-").to_ascii_lowercase()
}

// ---------------------------------------------------------------- lookup

pub fn t(key: &str) -> String {
    t_for(locale(), key)
}

pub fn t_for(locale: Locale, key: &str) -> String {
    if let Some(value) = catalog(locale).get(key) {
        return value.clone();
    }
    // 复数键误用单数接口时（common.files 而非 common.files.one）取 other，避免界面露出原始 key
    let plural_key = format!("{key}.other");
    if let Some(value) = catalog(locale).get(&plural_key) {
        return value.clone();
    }
    // 非中文界面缺键 → 回落中文，避免界面出现空白
    if locale != Locale::ZhCn {
        if let Some(value) = catalog(Locale::ZhCn).get(key) {
            return value.clone();
        }
        if let Some(value) = catalog(Locale::ZhCn).get(&plural_key) {
            return value.clone();
        }
    }
    if cfg!(debug_assertions) {
        eprintln!("[i18n] missing key: {key}");
    }
    key.to_string()
}

/// 静默查询：只返回存在的键（用于"有就覆盖、没有就保留原样"的场景，不产生缺键告警）
pub fn lookup(key: &str) -> Option<String> {
    let locale = locale();
    catalog(locale).get(key).cloned().or_else(|| {
        if locale != Locale::ZhCn {
            catalog(Locale::ZhCn).get(key).cloned()
        } else {
            None
        }
    })
}

pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    tf_for(locale(), key, args)
}

pub fn tf_for(locale: Locale, key: &str, args: &[(&str, &str)]) -> String {
    let mut text = t_for(locale, key);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// 复数：取 `key.one` / `key.other`（中文两句同文，英文按 n==1 区分）。
pub fn tn(key: &str, n: usize) -> String {
    tn_for(locale(), key, n)
}

pub fn tn_for(locale: Locale, key: &str, n: usize) -> String {
    let suffix = if n == 1 { "one" } else { "other" };
    let plural_key = format!("{key}.{suffix}");
    let template = if catalog(locale).contains_key(&plural_key) {
        t_for(locale, &plural_key)
    } else {
        t_for(locale, key)
    };
    template.replace("{n}", &n.to_string())
}

/// 替换模板里的 `{{k:web.x.y}}` 占位（生成静态网页 / 服务器页面用）。
pub fn render(template: &str, locale: Locale) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{k:") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 4..];
        match after.find("}}") {
            Some(end) => {
                let key = after[..end].trim();
                out.push_str(&t_for(locale, key));
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------- 结构化错误码

/// 错误码前缀：`[code:offline_repo]人类可读消息`。
///
/// 前端根据 code 分支、根据消息展示。这样控制流不依赖自然语言，
/// 切换界面语言后引导逻辑依然成立（曾经用 `msg.includes('离线创建')` 判断，一改语言就失效）。
pub const ERROR_CODE_PREFIX: &str = "[code:";

pub fn coded(code: &str, message: impl AsRef<str>) -> String {
    format!("{ERROR_CODE_PREFIX}{code}]{}", message.as_ref())
}

pub fn error_code(message: &str) -> Option<&str> {
    let rest = message.strip_prefix(ERROR_CODE_PREFIX)?;
    let end = rest.find(']')?;
    Some(&rest[..end])
}

/// 剥掉错误码，只留人类可读部分（展示用）
pub fn strip_error_code(message: &str) -> &str {
    match message.strip_prefix(ERROR_CODE_PREFIX).and_then(|rest| rest.find(']').map(|end| (rest, end))) {
        Some((rest, end)) => rest[end + 1..].trim_start(),
        None => message,
    }
}

// ---------------------------------------------------------------- 设置页数据

#[derive(Debug, Clone, Serialize)]
pub struct LocaleOption {
    pub tag: &'static str,
    pub native_name: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocaleInfo {
    /// 用户偏好："system" | "zh-CN" | "en-US"
    pub pref: String,
    /// 实际生效的语言 tag
    pub tag: String,
    /// 生效语言的母语名称（设置页展示）
    pub native_name: String,
    /// 系统语言 tag（探测失败时回落 zh-CN，恒有值）
    pub system_tag: String,
    /// 可选语言列表
    pub available: Vec<LocaleOption>,
}

pub fn info() -> LocaleInfo {
    let pref = pref();
    let locale = locale();
    let system = system_locale();
    LocaleInfo {
        pref: pref.as_str().to_string(),
        tag: locale.tag().to_string(),
        native_name: locale.native_name().to_string(),
        system_tag: system.tag().to_string(),
        available: Locale::all()
            .iter()
            .map(|l| LocaleOption { tag: l.tag(), native_name: l.native_name() })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(locale: Locale) -> std::collections::BTreeSet<String> {
        catalog(locale).keys().cloned().collect()
    }

    #[test]
    fn catalog_parses_and_is_not_empty() {
        assert!(catalog(Locale::ZhCn).len() > 20, "zh-CN catalog 太小");
        assert!(catalog(Locale::EnUs).len() > 20, "en-US catalog 太小");
        assert_eq!(catalog(Locale::ZhCn).get("common.save").map(String::as_str), Some("保存"));
        assert_eq!(catalog(Locale::EnUs).get("common.save").map(String::as_str), Some("Save"));
    }

    #[test]
    fn catalogs_have_identical_key_sets() {
        // web.* 只存在于 en-US（网页端按产品决策恒英文），因此豁免该前缀
        let strip = |set: std::collections::BTreeSet<String>| -> std::collections::BTreeSet<String> {
            set.into_iter().filter(|k| !k.starts_with("web.")).collect()
        };
        let zh = strip(keys(Locale::ZhCn));
        let en = strip(keys(Locale::EnUs));
        let missing_in_en: Vec<_> = zh.difference(&en).cloned().collect();
        let missing_in_zh: Vec<_> = en.difference(&zh).cloned().collect();
        assert!(missing_in_en.is_empty(), "en-US 缺少键: {missing_in_en:?}");
        assert!(missing_in_zh.is_empty(), "zh-CN 缺少键: {missing_in_zh:?}");
    }

    #[test]
    fn locale_parse_accepts_common_tags() {
        for tag in ["zh", "zh-CN", "zh_CN", "zh_CN.UTF-8", "ZH-cn", "zh-Hans-CN"] {
            assert_eq!(Locale::parse(tag), Some(Locale::ZhCn), "{tag}");
        }
        for tag in ["en", "en-US", "en_GB", "en-US.UTF-8"] {
            assert_eq!(Locale::parse(tag), Some(Locale::EnUs), "{tag}");
        }
        assert_eq!(Locale::parse("ja-JP"), None);
        assert_eq!(Locale::parse(""), None);
    }

    #[test]
    fn locale_pref_parsing() {
        assert_eq!(LocalePref::parse(None), LocalePref::System);
        assert_eq!(LocalePref::parse(Some("")), LocalePref::System);
        assert_eq!(LocalePref::parse(Some("system")), LocalePref::System);
        assert_eq!(LocalePref::parse(Some("garbage")), LocalePref::System);
        assert_eq!(LocalePref::parse(Some("en-US")), LocalePref::Fixed(Locale::EnUs));
        assert_eq!(LocalePref::parse(Some("zh-CN")), LocalePref::Fixed(Locale::ZhCn));
        assert_eq!(LocalePref::as_str(LocalePref::System), "system");
        assert_eq!(LocalePref::as_str(LocalePref::Fixed(Locale::EnUs)), "en-US");
    }

    #[test]
    fn resolve_priority_env_beats_pref() {
        assert_eq!(resolve_with(Some("en-US"), LocalePref::Fixed(Locale::ZhCn)), Locale::EnUs);
        assert_eq!(resolve_with(Some("bogus"), LocalePref::Fixed(Locale::EnUs)), Locale::EnUs);
        assert_eq!(resolve_with(None, LocalePref::Fixed(Locale::ZhCn)), Locale::ZhCn);
    }

    #[test]
    fn missing_key_falls_back_without_panic() {
        assert_eq!(t_for(Locale::EnUs, "no.such.key"), "no.such.key");
        assert_eq!(t_for(Locale::ZhCn, "no.such.key"), "no.such.key");
    }

    #[test]
    fn interpolation_and_plurals() {
        assert_eq!(tf_for(Locale::EnUs, "common.files", &[("n", "3")]), "3 files");
        assert_eq!(tn_for(Locale::EnUs, "common.files", 1), "1 file");
        assert_eq!(tn_for(Locale::EnUs, "common.files", 5), "5 files");
        assert_eq!(tn_for(Locale::ZhCn, "common.files", 5), "5 个文件");
    }

    /// clap 子命令的 about 是"运行时拼 key"（cli.cmd.{name}.about），扫描器看不到，
    /// 这里用真实子命令名兜底：重命名/新增子命令时这条测试会失败，提醒补文案。
    /// 防回归：Svelte 模板里的 t() 必须用 { } 包起来，
    /// 否则会被当作纯文本渲染（2026-11-03 截图 bug：设置页标题/标签页显示成 "t('common.settings')"）。
    #[test]
    fn svelte_markup_has_no_bare_t_calls() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/src");
        let mut bad = Vec::new();

        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "svelte") {
                    out.push(path);
                }
            }
        }

        let mut files = Vec::new();
        walk(&root, &mut files);

        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            // 只看 markup 区（</script> 之后）
            let markup = match text.find("</script>") {
                Some(i) => &text[i..],
                None => text.as_str(),
            };
            for (idx, line) in markup.lines().enumerate() {
                let trimmed = line.trim();
                let bare_text = trimmed.starts_with("t('") || line.contains(">t('");
                let bare_attr = line.contains("=\"t('") || line.contains("='t('");
                if bare_text || bare_attr {
                    bad.push(format!(
                        "{}:{} {}",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        idx + 1,
                        trimmed.chars().take(80).collect::<String>()
                    ));
                }
            }
        }

        assert!(
            bad.is_empty(),
            "以下模板把 t() 写成了裸文本（需要用 {{ }} 包裹）: {bad:#?}"
        );
    }

    #[test]
    fn cli_subcommand_abouts_exist() {
        for name in [
            "init", "record", "push", "pull", "rollback", "restore", "status", "log",
            "config", "export", "serve", "deploy", "download-blender", "list-blender-versions",
        ] {
            let key = format!("cli.cmd.{name}.about");
            assert!(
                catalog(Locale::ZhCn).contains_key(&key),
                "zh-CN 缺少 {key}"
            );
            assert!(
                catalog(Locale::EnUs).contains_key(&key),
                "en-US 缺少 {key}"
            );
        }
        assert!(!catalog(Locale::ZhCn).get("cli.about").unwrap().is_empty());
    }

    #[test]
    fn lib_tests_are_locale_stable() {
        // 单元测试固定中文（见 locale() 里的 #[cfg(test)] 分支）
        assert_eq!(locale(), Locale::ZhCn);
        assert_eq!(t("common.save"), "保存");
    }

    #[test]
    fn error_code_roundtrip() {
        let msg = coded("offline_repo", "该项目为离线创建");
        assert_eq!(error_code(&msg), Some("offline_repo"));
        assert_eq!(strip_error_code(&msg), "该项目为离线创建");
        // 没有码 → 原样
        assert_eq!(error_code("普通错误"), None);
        assert_eq!(strip_error_code("普通错误"), "普通错误");
        // 前缀残缺不被误判
        assert_eq!(error_code("[code:broken"), None);
    }

    #[test]
    fn render_replaces_placeholders() {
        let out = render("<h1>{{k:common.save}}</h1><p>{{k:web.missing}}</p>", Locale::EnUs);
        assert_eq!(out, "<h1>Save</h1><p>web.missing</p>");
        assert_eq!(render("no placeholder", Locale::ZhCn), "no placeholder");
        assert_eq!(render("{{k:common.save", Locale::EnUs), "{{k:common.save");
    }

    /// 扫描 src/**.rs 里出现的 catalog 形状字符串，断言键都存在。
    /// 防止 P1/P2 大批量抽取时拼错键（Rust 没有编译期 key 校验）。
    #[test]
    fn rust_source_keys_exist_in_catalog() {
        const SURFACES: [&str; 5] = ["common.", "gui.", "cli.", "err.", "web."];
        let mut checked = 0usize;
        let mut missing: Vec<String> = Vec::new();

        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = rust_files(manifest.join("src"));
        files.extend(rust_files(manifest.join("src-tauri/src")));

        for path in files {
            if path.ends_with("i18n.rs") {
                continue; // 本文件里的字符串是测试夹具与文档
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            for literal in string_literals(&text) {
                if !SURFACES.iter().any(|s| literal.starts_with(s)) {
                    continue;
                }
                // 只认 "a.b.c" 形状，避免误判用户可见的普通文案；
                // 含 { } 的是运行时拼出来的动态键（如 cli.cmd.{name}.about），交给下面的专项测试
                if !literal.contains('.') || literal.contains(' ') || literal.contains('{') {
                    continue;
                }
                checked += 1;
                let plural_base = format!("{literal}.one");
                let present = catalog(Locale::ZhCn).contains_key(&literal)
                    || catalog(Locale::ZhCn).contains_key(&plural_base)
                    || catalog(Locale::EnUs).contains_key(&literal)
                    || catalog(Locale::EnUs).contains_key(&plural_base);
                if !present {
                    missing.push(format!("{}: {literal}", path.display()));
                }
            }
        }

        assert!(missing.is_empty(), "以下 key 不在 catalog 里: {missing:#?}");
        // 允许为 0（P0 阶段还没有抽取），只保证"出现即存在"
        let _ = checked;
    }

    fn rust_files(dir: std::path::PathBuf) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&dir) else { return out };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(rust_files(path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
        out
    }

    /// 提取源码里的字符串字面量内容（跳过注释行；处理 " 转义）
    fn string_literals(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let bytes: Vec<char> = line.chars().collect();
            let mut i = 0usize;
            while i < bytes.len() {
                if bytes[i] != '"' {
                    i += 1;
                    continue;
                }
                // 前面是 r# 之类原始字符串前缀时不处理（不常见于 key）
                let mut buf = String::new();
                let mut j = i + 1;
                while j < bytes.len() {
                    let ch = bytes[j];
                    // 92 = 反斜杠：跳过转义字符（不用字面量，避免源码里的反斜杠被工具链吃掉）
                    if ch == (92 as char) {
                        j += 2;
                        continue;
                    }
                    if ch == '"' {
                        break;
                    }
                    buf.push(ch);
                    j += 1;
                }
                out.push(buf);
                i = j + 1;
            }
        }
        out
    }
}
