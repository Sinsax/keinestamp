//! 关于页（设置 → 关于）的元信息：程序版本、Lore 运行时、关键路径与更新检查。
//!
//! 更新检查只访问公开的 GitHub Releases；离线 / 限流 / 仓库暂无 release 一律按
//! 「未知」如实返回，既不阻塞 UI，也不当作错误弹窗——用户主动点「检查更新」才发起。

use serde::Serialize;
use std::time::Duration;

pub const APP_NAME: &str = "Keinestamp";
/// 与 Cargo.toml / tauri.conf.json 同源（构建时注入，前端不再另维护版本号）
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROJECT_URL: &str = "https://github.com/Sinsax/keinestamp";
pub const RELEASES_URL: &str = "https://github.com/Sinsax/keinestamp/releases";
pub const LICENSE: &str = "MIT";

const RELEASE_API: &str = "https://api.github.com/repos/Sinsax/keinestamp/releases/latest";
const RELEASE_LATEST_PAGE: &str = "https://github.com/Sinsax/keinestamp/releases/latest";
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);
const USER_AGENT: &str = concat!("Keinestamp/", env!("CARGO_PKG_VERSION"));

/// 更新检查结论。Unknown 是正常状态（离线 / 接口限流 / 远端没有 release），不是错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    /// 远端没有更高的版本
    UpToDate,
    /// 远端存在更高的版本
    Available,
    /// 无法判断
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub state: UpdateState,
    /// 当前运行的版本（不带 v 前缀）
    pub current: String,
    /// 远端最新版本（不带 v 前缀）；无法获取时为 None
    pub latest: Option<String>,
    /// 下载 / 发布页 URL（有新版时指向具体 tag）
    pub url: String,
    /// 面向用户的一句话说明
    pub message: String,
    /// 本次检查时间（RFC3339，UTC）
    pub checked_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AboutInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub platform: String,
    /// 捆绑 / 要求的 Lore 版本（精确匹配，见 lore_backend::LORE_MIN_VERSION）
    pub lore_expected: &'static str,
    pub project_url: &'static str,
    pub license: &'static str,
    /// 捆绑 Lore 的版本号（lore --version 实测）；检测失败时为 None（原因见 lore_error）
    pub lore: Option<String>,
    pub lore_error: Option<String>,
    /// 程序目录（便携部署基准：serve.toml / lore-bin / lore-server-store 都在这里）
    pub exe_dir: String,
    /// 全局配置目录（app-config.toml 所在目录）
    pub config_dir: String,
}

/// 解析版本号：容忍 v0.6.1 / 0.6.1 / 0.6.1-beta.1 / 0.6.1+373，只看数字段。
fn parse_version(raw: &str) -> Option<Vec<u64>> {
    let s = raw.trim();
    let s = s
        .strip_prefix('v')
        .or_else(|| s.strip_prefix('V'))
        .unwrap_or(s);
    let core: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if core.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for part in core.split('.') {
        if part.is_empty() {
            return None;
        }
        parts.push(part.parse::<u64>().ok()?);
    }
    Some(parts)
}

/// 远端版本是否比当前版本新。任一版本无法解析 → None（调用方按「未知」处理）。
pub fn is_newer(latest: &str, current: &str) -> Option<bool> {
    let mut a = parse_version(latest)?;
    let mut b = parse_version(current)?;
    let n = a.len().max(b.len());
    // 补齐到同长度再逐位比较，避免 "0.6" 与 "0.5.9" 因长度不同误判
    a.resize(n, 0);
    b.resize(n, 0);
    Some(a > b)
}

/// 从 GitHub Releases API 响应里取 tag_name
fn tag_from_release_json(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let tag = v.get("tag_name")?.as_str()?.trim();
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_string())
    }
}

/// 从 .../releases/tag/v0.6.1 这类跳转地址里取 tag
fn tag_from_release_url(url: &str) -> Option<String> {
    let (_, tail) = url.rsplit_once("/tag/")?;
    let tag = tail
        .split(|c| c == '/' || c == '?' || c == '#')
        .next()?
        .trim();
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_string())
    }
}

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
}

/// 取远端最新 tag。先走 API，失败（限流 / 404 / 网络）再退回 releases/latest 的跳转地址。
fn fetch_latest_tag() -> Result<String, String> {
    let agent = http_agent();
    let api_err = match agent
        .get(RELEASE_API)
        .set("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(resp) => match resp.into_string() {
            Ok(body) => match tag_from_release_json(&body) {
                Some(tag) => return Ok(tag),
                None => crate::i18n::t("err.about.missingTag"),
            },
            Err(e) => crate::i18n::tf("err.about.readFailed", &[("error", &e.to_string())]),
        },
        Err(ureq::Error::Status(code, _)) => crate::i18n::tf("err.about.httpStatus", &[("code", &code.to_string())]),
        Err(e) => crate::i18n::tf("err.about.requestFailed", &[("error", &e.to_string())]),
    };

    match agent.get(RELEASE_LATEST_PAGE).call() {
        Ok(resp) => tag_from_release_url(resp.get_url())
            .ok_or_else(|| crate::i18n::tf("err.about.releaseNotRedirected", &[("message", api_err.as_str())])),
        Err(e) => Err(crate::i18n::tf("err.about.releaseRequestFailed", &[("message", api_err.as_str()), ("error", &e.to_string())])),
    }
}

/// 检查更新。永不返回 Err：失败也给出可展示的 Unknown 结果。
pub fn check_update() -> UpdateInfo {
    let current = APP_VERSION.to_string();
    let checked_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let tag = match fetch_latest_tag() {
        Ok(tag) => tag,
        Err(e) => {
            return UpdateInfo {
                state: UpdateState::Unknown,
                current,
                latest: None,
                url: RELEASES_URL.to_string(),
                message: e,
                checked_at,
            };
        }
    };
    let latest = tag
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string();
    match is_newer(&tag, APP_VERSION) {
        Some(true) => UpdateInfo {
            state: UpdateState::Available,
            current: current.clone(),
            latest: Some(latest.clone()),
            url: format!("{}/tag/{}", RELEASES_URL, tag),
            message: crate::i18n::tf("err.about.found", &[("latest", latest.as_str()), ("current", current.as_str())]),
            checked_at,
        },
        Some(false) => UpdateInfo {
            state: UpdateState::UpToDate,
            current: current.clone(),
            latest: Some(latest),
            url: RELEASES_URL.to_string(),
            message: crate::i18n::tf("err.about.upToDate", &[("current", current.as_str())]),
            checked_at,
        },
        None => UpdateInfo {
            state: UpdateState::Unknown,
            current,
            latest: None,
            url: RELEASES_URL.to_string(),
            message: crate::i18n::tf("err.about.cannotCompare", &[("tag", tag.as_str())]),
            checked_at,
        },
    }
}

/// 汇总关于页数据（版本、Lore 运行时、程序/配置目录）。
pub fn about_info() -> AboutInfo {
    let (lore, lore_error) = match crate::lore_backend::check_lore_env() {
        Ok(info) => (Some(info.version), None),
        Err(e) => (None, Some(e.to_string())),
    };
    let exe_dir = crate::app_config::exe_dir().display().to_string();
    let config_path = crate::app_config::default_app_config_path();
    let config_dir = config_path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| config_path.display().to_string());
    AboutInfo {
        name: APP_NAME,
        version: APP_VERSION,
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        lore_expected: crate::lore_backend::LORE_MIN_VERSION,
        project_url: PROJECT_URL,
        license: LICENSE,
        lore,
        lore_error,
        exe_dir,
        config_dir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_prefixed_versions() {
        assert_eq!(parse_version("0.5.0"), Some(vec![0, 5, 0]));
        assert_eq!(parse_version("v0.5.0"), Some(vec![0, 5, 0]));
        assert_eq!(parse_version("V0.6"), Some(vec![0, 6]));
        assert_eq!(parse_version("0.6.1-beta.1"), Some(vec![0, 6, 1]));
        assert_eq!(parse_version("0.8.6+373"), Some(vec![0, 8, 6]));
        assert_eq!(parse_version("  v1.0.0  "), Some(vec![1, 0, 0]));
        assert_eq!(parse_version("nightly"), None);
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("v"), None);
        assert_eq!(parse_version("1..2"), None);
    }

    #[test]
    fn compares_versions_numerically_not_lexically() {
        // 0.10.0 > 0.9.9（按字符串比较会判反）
        assert_eq!(is_newer("v0.10.0", "0.9.9"), Some(true));
        assert_eq!(is_newer("0.5.0", "0.5.0"), Some(false));
        assert_eq!(is_newer("0.5", "0.5.0"), Some(false));
        assert_eq!(is_newer("0.5.1", "0.5"), Some(true));
        assert_eq!(is_newer("v0.5.1-beta.2", "0.5.0"), Some(true));
        assert_eq!(is_newer("0.4.9", "0.5.0"), Some(false));
        assert_eq!(is_newer("nightly", "0.5.0"), None);
    }

    #[test]
    fn extracts_tag_from_release_url() {
        assert_eq!(
            tag_from_release_url("https://github.com/Sinsax/keinestamp/releases/tag/v0.6.0"),
            Some("v0.6.0".to_string())
        );
        assert_eq!(
            tag_from_release_url("https://github.com/Sinsax/keinestamp/releases/tag/v0.6.0?x=1"),
            Some("v0.6.0".to_string())
        );
        assert_eq!(
            tag_from_release_url("https://github.com/Sinsax/keinestamp/releases"),
            None
        );
    }

    #[test]
    fn extracts_tag_from_release_json() {
        let body = r#"{"tag_name":"v0.6.1","name":"v0.6.1","html_url":"https://x"}"#;
        assert_eq!(tag_from_release_json(body), Some("v0.6.1".to_string()));
        assert_eq!(tag_from_release_json(r#"{"tag_name":"  "}"#), None);
        assert_eq!(tag_from_release_json(r#"{"message":"Not Found"}"#), None);
        assert_eq!(tag_from_release_json("not json"), None);
    }

    /// 真实网络检查（默认忽略）：确认 GitHub API / 跳转兜底在当前环境可用。
    /// 运行：cargo test --lib about -- --ignored live_check_update --nocapture
    #[test]
    #[ignore = "需要联网：GitHub Releases 可达时验证真实响应"]
    fn live_check_update() {
        let info = check_update();
        eprintln!("[live] {:?}", info);
        assert_eq!(info.current, APP_VERSION);
        assert!(info.message.contains("版本"));
        // 联网成功时一定拿到 latest；离线/限流则如实返回 unknown（不当作失败）
        if info.state != UpdateState::Unknown {
            assert!(
                info.latest.is_some(),
                "非 unknown 时必须带 latest: {:?}",
                info
            );
            assert!(info.url.starts_with("https://"));
            assert!(!info.checked_at.is_empty());
        }
    }

}
