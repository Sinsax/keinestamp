use crate::three_assets;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime};
use tiny_http::{Header, Method, Response, Server};

const MAX_UPLOAD_SIZE: usize = 512 * 1024 * 1024;
const MAX_EXTRACT_SIZE: u64 = 512 * 1024 * 1024;
const BODY_READ_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_LOGIN_BODY: u64 = 8 * 1024;
const MAX_AUTH_FAILURES: u32 = 10;
const AUTH_WINDOW_SECS: u64 = 60;
const AUTH_BLOCK_SECS: u64 = 120;
const MAX_CONCURRENT_REQUESTS: usize = 32;

// --- Logger ---

fn log_ts() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn log_info(tag: &str, msg: &str) {
    eprintln!("[{}] [INFO] [{}] {}", log_ts(), tag, msg);
}

fn log_warn(tag: &str, msg: &str) {
    eprintln!("[{}] [WARN] [{}] {}", log_ts(), tag, msg);
}

fn log_error(tag: &str, msg: &str) {
    eprintln!("[{}] [ERROR] [{}] {}", log_ts(), tag, msg);
}

static TRUST_PROXY: OnceLock<bool> = OnceLock::new();
static SITE_NAME: OnceLock<String> = OnceLock::new();
static WEB_ROOT: OnceLock<PathBuf> = OnceLock::new();
static UPLOAD_SEQ: AtomicUsize = AtomicUsize::new(0);

fn request_ip(request: &tiny_http::Request) -> IpAddr {
    let fallback = request
        .remote_addr()
        .map(|sa| sa.ip())
        .unwrap_or(IpAddr::from([127, 0, 0, 1]));
    fallback
}

// 反代（Caddy/nginx/CF）场景：serve.toml trust_proxy=true 时取 X-Forwarded-For 第一个 IP（真实客户端）。
// 默认关闭——直接暴露时 X-Forwarded-For 可伪造，切勿开启。
fn request_ip_trusted(request: &tiny_http::Request) -> IpAddr {
    if *TRUST_PROXY.get().unwrap_or(&false) {
        if let Some(h) = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("X-Forwarded-For"))
        {
            if let Some(first) = h.value.as_str().split(',').next() {
                if let Ok(ip) = first.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }
    request_ip(request)
}

// 客户端是否走 HTTPS——用于登录 cookie 动态 Secure：
// 反代场景（trust_proxy=true）读 X-Forwarded-Proto（Caddy 需显式 header_up X-Forwarded-Proto https，
// 因为 Caddy 对 cloudflared 的回源是 http，默认会写 http）；直连场景按 TLS 连接判断。
// 效果：公网（HTTPS）cookie 带 Secure（http 请求不发 cookie，堵住 301 跳转前的明文盲区），
// 局域网 HTTP 直连不带 Secure（保持登录体验）。
fn request_is_https(request: &tiny_http::Request) -> bool {
    if *TRUST_PROXY.get().unwrap_or(&false) {
        if let Some(h) = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("X-Forwarded-Proto"))
        {
            if let Some(first) = h.value.as_str().split(',').next() {
                if first.trim().eq_ignore_ascii_case("https") {
                    return true;
                }
            }
        }
        return false;
    }
    request.secure()
}

// --- Serve config ---

#[derive(Debug, Clone, Deserialize)]
pub struct ServeConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub deploy_token: Option<String>,
    pub web_root: PathBuf,
    #[serde(default)]
    pub trust_proxy: bool,
    /// 备份服务器开关：serve 启动时探测 41339 health，离线则拉起 loreserver（持久化 store，跟随 serve 生命周期）
    #[serde(default)]
    pub enable_loreserver: bool,
    /// 站点名称：首页大标题默认 Keinestamp，可自定义
    #[serde(default)]
    pub site_name: Option<String>,
}

// 默认只监听本机：请使用 Caddy 反代对外提供服务；
// 仅局域网直接连接时，请改成 0.0.0.0（明文 HTTP，风险自负）。
fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    7799
}

pub fn load_config(path: &Path) -> Result<ServeConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read serve config {:?}", path))?;
    let mut config: ServeConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse serve config {:?}", path))?;

    if config.deploy_token.is_none() {
        anyhow::bail!("serve.toml missing `deploy_token`");
    }
    if config.web_root.as_os_str().is_empty() {
        anyhow::bail!("serve.toml missing `web_root`");
    }

    // web_root 相对路径 → 相对程序所在目录（serve.toml 随 exe 目录拷贝即用，便携部署）
    if !config.web_root.is_absolute() {
        config.web_root = crate::app_config::exe_dir().join(&config.web_root);
    }

    Ok(config)
}

// --- Rate limiter ---

struct AttemptTracker {
    count: u32,
    window_start: Instant,
    blocked_until: Option<Instant>,
}

struct RateLimiter {
    attempts: Mutex<HashMap<IpAddr, AttemptTracker>>,
}

struct SlotGuard(Arc<AtomicUsize>);

impl Drop for SlotGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
        }
    }

    fn check(&self, ip: &IpAddr) -> bool {
        let mut map = self.attempts.lock().unwrap();
        let now = Instant::now();

        map.retain(|_, tracker| {
            if let Some(blocked_until) = tracker.blocked_until {
                now < blocked_until
            } else if now.duration_since(tracker.window_start)
                > Duration::from_secs(AUTH_WINDOW_SECS)
            {
                false
            } else {
                true
            }
        });

        if let Some(tracker) = map.get(ip) {
            if let Some(blocked_until) = tracker.blocked_until {
                if now < blocked_until {
                    return false;
                }
            }
            if now.duration_since(tracker.window_start) > Duration::from_secs(AUTH_WINDOW_SECS) {
                map.remove(ip);
                return true;
            }
            if tracker.count >= MAX_AUTH_FAILURES {
                let count = tracker.count;
                let window_start = tracker.window_start;
                map.insert(
                    *ip,
                    AttemptTracker {
                        count,
                        window_start,
                        blocked_until: Some(now + Duration::from_secs(AUTH_BLOCK_SECS)),
                    },
                );
                return false;
            }
        }
        true
    }

    fn record_failure(&self, ip: &IpAddr) {
        let mut map = self.attempts.lock().unwrap();
        let now = Instant::now();

        let tracker = map.entry(*ip).or_insert(AttemptTracker {
            count: 0,
            window_start: now,
            blocked_until: None,
        });

        if now.duration_since(tracker.window_start) > Duration::from_secs(AUTH_WINDOW_SECS) {
            tracker.count = 0;
            tracker.window_start = now;
        }
        tracker.count += 1;
    }

    fn record_success(&self, ip: &IpAddr) {
        let mut map = self.attempts.lock().unwrap();
        map.remove(ip);
    }
}

// --- Auth ---

enum AuthResult {
    Authenticated,
    RateLimited,
    Unauthorized,
}

fn check_deploy_auth(
    request: &tiny_http::Request,
    deploy_token: Option<&str>,
    rate_limiter: &RateLimiter,
) -> AuthResult {
    let ip = request_ip_trusted(request);
    if !rate_limiter.check(&ip) {
        return AuthResult::RateLimited;
    }

    let auth_header = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"));
    let auth_value = match auth_header {
        Some(h) => h.value.as_str(),
        None => {
            rate_limiter.record_failure(&ip);
            return AuthResult::Unauthorized;
        }
    };

    if auth_value.starts_with("Bearer ") {
        let token = auth_value.trim_start_matches("Bearer ");
        if deploy_token.map_or(false, |t| t == token) {
            rate_limiter.record_success(&ip);
            return AuthResult::Authenticated;
        }
    }

    rate_limiter.record_failure(&ip);
    AuthResult::Unauthorized
}

fn unauthorized_response() -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string("401 Unauthorized")
        .with_status_code(401)
        .with_header(Header::from_bytes(&b"WWW-Authenticate"[..], &b"Bearer"[..]).unwrap())
}

// --- Cookie auth for browser ---

fn get_cookie<'a>(request: &'a tiny_http::Request, name: &str) -> Option<&'a str> {
    let cookie_header = request.headers().iter().find(|h| h.field.equiv("Cookie"))?;
    for pair in cookie_header.value.as_str().split(';') {
        let pair = pair.trim();
        if let Some(pos) = pair.find('=') {
            if pair[..pos].trim() == name {
                return Some(pair[pos + 1..].trim());
            }
        }
    }
    None
}

fn check_browser_static_auth(
    request: &tiny_http::Request,
    rate_limiter: &RateLimiter,
    project_name: &str,
    project_tokens: &RwLock<HashMap<String, String>>,
) -> AuthResult {
    let ip = request_ip_trusted(request);
    if !rate_limiter.check(&ip) {
        return AuthResult::RateLimited;
    }

    let auth_header = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"));

    if let Some(h) = auth_header {
        let value = h.value.as_str();
        if value.starts_with("Bearer ") {
            let token = value.trim_start_matches("Bearer ");
            if project_tokens
                .read()
                .unwrap()
                .get(project_name)
                .map_or(false, |t| t == token)
            {
                rate_limiter.record_success(&ip);
                return AuthResult::Authenticated;
            }
            // Wrong bearer → fall through to cookie check before failing
        }
    }

    if let Some(cookie_token) = get_cookie(request, "keinestamp_token") {
        if project_tokens
            .read()
            .unwrap()
            .get(project_name)
            .map_or(false, |t| t == cookie_token)
        {
            rate_limiter.record_success(&ip);
            return AuthResult::Authenticated;
        }
    }

    rate_limiter.record_failure(&ip);
    AuthResult::Unauthorized
}

// --- Login page ---

fn login_page_html(project_name: &str) -> String {
    format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>Keinestamp 登录</title>
<script>document.documentElement.dataset.theme = localStorage.getItem('keinestamp_web_theme') === 'night' ? 'night' : 'day';</script>
<style>
:root{{
--bg:#f2f4f7;--surface:#ffffff;--border:#e3e6ea;--border-strong:#d3d8de;--text:#191d23;--text-dim:#6b7480;
--accent:#dc4a44;--accent-hover:#c93d38;--accent-soft:#fbeeed;
--shadow-panel:0 1px 2px rgba(20,24,30,0.04),0 6px 20px rgba(20,24,30,0.05);
--shadow-lg:0 12px 40px rgba(20,24,30,0.16);
}}
[data-theme="night"]{{
--bg:#101114;--surface:#1a1c1f;--border:#26292d;--border-strong:#34383d;--text:#e6e8eb;--text-dim:#8f96a0;
--accent:#e05a55;--accent-hover:#d14b46;--accent-soft:#3d2628;
--shadow-panel:0 1px 2px rgba(0,0,0,0.25),0 6px 20px rgba(0,0,0,0.3);
--shadow-lg:0 12px 40px rgba(0,0,0,0.5);
}}
*{{box-sizing:border-box}}
body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,"PingFang SC","Microsoft YaHei",sans-serif;display:flex;justify-content:center;align-items:center;min-height:100vh;margin:0;background:var(--bg);color:var(--text)}}
.card{{background:var(--surface);border:1px solid var(--border);padding:34px 32px 28px;border-radius:14px;box-shadow:var(--shadow-panel);text-align:center;width:330px}}
h1{{font-size:18px;margin:0 0 4px;font-weight:700;color:var(--text);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
.sub{{font-size:12.5px;color:var(--text-dim);margin-bottom:22px}}
input{{width:100%;padding:9px 13px;border:1px solid var(--border-strong);border-radius:8px;background:var(--surface);color:var(--text);font-size:13.5px;outline:none;transition:border-color .15s,box-shadow .15s}}
input:focus{{border-color:var(--accent);box-shadow:0 0 0 2.5px color-mix(in srgb,var(--accent) 16%,transparent)}}
button{{margin-top:14px;padding:9px 32px;border:none;border-radius:8px;background:var(--accent);color:#fff;font-size:13.5px;font-weight:600;cursor:pointer;width:100%;transition:background .15s}}
button:hover{{background:var(--accent-hover)}}
.error{{color:var(--accent);font-size:12.5px;margin-top:12px;display:none}}
</style>
<link rel="icon" href="data:image/png;base64,{favicon}">
</head>
<body>
<div class="card">
  <h1>{project}</h1>
  <div class="sub">输入访问令牌以查看时间线</div>
  <form method="post" action="/{project}/login">
    <input type="password" name="token" placeholder="访问令牌" required autofocus id="token-input" />
    <button type="submit">确认</button>
    <div class="error" id="error">令牌无效</div>
  </form>
</div>
<script>
(function(){{
  var key = 'keinestamp_token_{project}';
  var saved = localStorage.getItem(key);
  if (saved) {{
    document.getElementById('token-input').value = saved;
  }}
  var url = new URL(window.location.href);
  if (url.searchParams.has('error')) document.getElementById('error').style.display='block';
}})();
</script>
</body>
</html>"##,
        project = project_name,
        favicon = crate::three_assets::logo_favicon_b64()
    )
}

fn serve_login_page(request: tiny_http::Request, project_name: &str) -> Result<()> {
    let hdr = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
    let response = Response::from_string(login_page_html(project_name)).with_header(hdr);
    request.respond(response)?;
    Ok(())
}

fn handle_index(request: tiny_http::Request) -> Result<()> {
    let hdr = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
    let response = Response::from_string(index_page_html()).with_header(hdr);
    request.respond(response)?;
    Ok(())
}

fn index_page_html() -> String {
    let site_name = SITE_NAME
        .get()
        .cloned()
        .unwrap_or_else(|| "Keinestamp".to_string());
    INDEX_HTML_TEMPLATE
        .replace("{favicon}", crate::three_assets::logo_favicon_b64())
        .replace(
            "{greenfavicon}",
            crate::three_assets::logo_green_favicon_b64(),
        )
        .replace("{site_name}", &site_name)
}

const INDEX_HTML_TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1.0">
<link rel="icon" href="data:image/png;base64,{favicon}">
<title>Keinestamp</title>
<script>
  document.documentElement.dataset.theme = localStorage.getItem('keinestamp_web_theme') === 'night' ? 'night' : 'day'
</script>
<style>
:root {
  --bg: #f2f4f7;
  --surface: #ffffff;
  --text: #191d23;
  --text-dim: #6b7480;
  --line: #e3e6ea;
  --accent: #dc4a44;
  --tint: #4f8ec4;
  --tint-soft: #edf4fb;
  --deco-a: #9ab3cd;
  --deco-b: #8fa694;
  --deco-c: #c7a9a9;
  --radius-lg: 14px;
  --shadow-panel: 0 1px 2px rgba(20, 24, 30, 0.04), 0 6px 20px rgba(20, 24, 30, 0.05);
  --shadow-lg: 0 12px 40px rgba(20, 24, 30, 0.16);
  --shadow-sm: 0 1px 2px rgba(20, 24, 30, 0.05);
}
[data-theme="night"] {
  --bg: #101114;
  --surface: #1a1c1f;
  --text: #e6e8eb;
  --text-dim: #8f96a0;
  --line: #26292d;
  --accent: #e05a55;
  --tint: #5b9ecf;
  --tint-soft: #1f2c38;
  --deco-a: #6b7478;
  --deco-b: #6e8478;
  --deco-c: #8a6e6e;
  --shadow-panel: 0 1px 2px rgba(0, 0, 0, 0.25), 0 6px 20px rgba(0, 0, 0, 0.3);
  --shadow-lg: 0 12px 40px rgba(0, 0, 0, 0.5);
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
}
* { box-sizing: border-box; }
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Microsoft YaHei", sans-serif;
  margin: 0;
  min-height: 100vh;
  background: var(--bg);
  color: var(--text);
  overflow-x: hidden;
  animation: fade-in 0.4s ease;
}
@keyframes fade-in { from { opacity: 0 } to { opacity: 1 } }

/* ---- 装饰层 ---- */
.deco { position: fixed; pointer-events: none; }
.glow-tl { width: 420px; height: 420px; left: -140px; top: -140px; border-radius: 50%; background: radial-gradient(circle, color-mix(in srgb, var(--deco-a) 14%, transparent), transparent 70%); filter: blur(40px); }
.glow-br { width: 460px; height: 460px; right: -160px; bottom: -160px; border-radius: 50%; background: radial-gradient(circle, color-mix(in srgb, var(--tint) 22%, transparent), transparent 70%); filter: blur(40px); }
[data-theme="night"] .glow-br { background: radial-gradient(circle, color-mix(in srgb, var(--tint) 5%, transparent), transparent 70%); }
.wave { top: 0; left: 0; right: 0; height: 140px; opacity: 0.07; }
.wave svg { width: 100%; height: 100%; }
.code-tex { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 10px; color: var(--tint); opacity: 0.05; letter-spacing: 0.5px; }
.c1 { left: 4%; bottom: 14%; }
.c2 { right: 5%; bottom: 40%; }
.c3 { left: 32%; bottom: 6%; }
.c4 { right: 10%; top: 30%; }

/* ---- 顶部品牌条 ---- */
.topbar {
  display: flex; align-items: center; justify-content: space-between;
  padding: 26px 48px; position: relative; z-index: 2;
}
.brand-mini { display: flex; align-items: center; gap: 10px; font-size: 14px; font-weight: 700; letter-spacing: 0.3px; }
.brand-mini .bm-logo { width: 26px; height: 26px; border-radius: 7px; }
.topbar-right { font-size: 11px; color: var(--text-dim); letter-spacing: 2px; text-transform: uppercase; }
.theme-btn {
  width: 30px; height: 30px; padding: 0;
  display: flex; align-items: center; justify-content: center;
  background: none; border: 1px solid var(--line); color: var(--text-dim);
  border-radius: 8px; cursor: pointer;
  transition: color 0.12s, border-color 0.12s, background 0.12s;
}
.theme-btn svg { width: 15px; height: 15px; display: block; }
.theme-btn .icon-moon { display: none; }
[data-theme="night"] .theme-btn .icon-sun { display: none; }
[data-theme="night"] .theme-btn .icon-moon { display: block; }
.theme-btn:hover { color: var(--accent); border-color: var(--accent); background: color-mix(in srgb, var(--accent) 6%, transparent); }

/* ---- Hero 区：左文右画 ---- */
.hero {
  display: grid;
  grid-template-columns: 1.15fr 0.85fr;
  align-items: center;
  min-height: 58vh;
  padding: 0 60px;
  gap: 40px;
  position: relative; z-index: 2;
}
.eyebrow {
  font-size: 10px; font-weight: 600; letter-spacing: 4px; color: var(--text-dim);
  text-transform: uppercase; margin-bottom: 18px;
  display: flex; align-items: center; gap: 10px;
}
.eyebrow::before {
  content: ""; width: 28px; height: 1px; background: var(--accent);
}
.eyebrow .dot { color: var(--accent); }
h1 {
  font-size: clamp(44px, 6.5vw, 72px);
  font-weight: 750;
  letter-spacing: -2px;
  margin: 0 0 10px;
  line-height: 1;
}
.hero-sub { font-size: 15px; color: var(--text-dim); margin: 0 0 26px; letter-spacing: 0.3px; max-width: 36ch; }
/* ---- 彩蛋特效层 ---- */
/* 月晕脉冲：月亮本色的柔光自月体扩散，同时月体轻弹（强度收敛） */
.moon-halo {
  position: absolute; left: 50%; top: 50%; width: 180px; height: 180px;
  margin: -90px 0 0 -90px; border-radius: 50%; pointer-events: none;
  background: radial-gradient(circle, color-mix(in srgb, var(--tint) 18%, transparent), transparent 60%);
  animation: halo-pulse 0.9s cubic-bezier(0.25, 0.6, 0.3, 1) forwards;
}
@keyframes halo-pulse {
  0% { opacity: 0; transform: scale(0.55); }
  30% { opacity: 0.55; }
  100% { opacity: 0; transform: scale(1.3); }
}
.ha-moon svg { transform: translate(var(--gx, 0px), var(--gy, 0px)); }
.moon-pop { animation: moon-pop 0.5s cubic-bezier(0.25, 0.6, 0.3, 1); }
@keyframes moon-pop {
  0% { transform: translate(var(--gx, 0px), var(--gy, 0px)) scale(1); }
  35% { transform: translate(var(--gx, 0px), var(--gy, 0px)) scale(1.025); }
  100% { transform: translate(var(--gx, 0px), var(--gy, 0px)) scale(1); }
}
.ha-moon { will-change: transform; }
.ha-moon:hover { animation: moon-wobble 1.6s ease-in-out infinite; }
@keyframes moon-wobble {
  0%, 100% { transform: rotate(-2.5deg); }
  50% { transform: rotate(2.5deg); }
}
/* 浮游星尘：月夜里缓缓上浮的微光 */
.mote {
  position: fixed; bottom: -10px; border-radius: 50%;
  background: var(--tint); opacity: 0;
  pointer-events: none; z-index: 0;
  animation: mote-rise linear infinite;
}
@keyframes mote-rise {
  0% { transform: translate(0, 0); opacity: 0; }
  12% { opacity: var(--mo, 0.3); }
  82% { opacity: var(--mo, 0.3); }
  100% { transform: translate(var(--mx, 0px), -106vh); opacity: 0; }
}
/* 低语：偶尔浮过一句没有来由的话 */
.whisper {
  position: fixed; z-index: 55;
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  font-size: 10.5px; letter-spacing: 3px; color: var(--text-dim);
  opacity: 0; pointer-events: none;
  animation: whisper-in 5.4s cubic-bezier(0.25, 0.6, 0.3, 1) forwards;
}
@keyframes whisper-in {
  0% { opacity: 0; transform: translateY(8px); }
  16% { opacity: 0.5; }
  70% { opacity: 0.45; }
  100% { opacity: 0; transform: translateY(-8px); }
}
/* 刻度呼吸：时间轴刻度错拍起伏 */
.cf-scale b { transform-origin: 50% 100%; animation: scale-breathe 3.4s ease-in-out infinite; }
@keyframes scale-breathe {
  0%, 100% { transform: scaleY(1); }
  50% { transform: scaleY(1.45); }
}
@media (prefers-reduced-motion: reduce) {
  .moon-halo { animation-duration: 0.01s; }
  .ha-moon:hover, .cf-scale b { animation: none; }
  .mote { display: none; }
}
.hero-code {
  display: inline-block;
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  font-size: 12px;
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 7%, transparent);
  border: 1px solid color-mix(in srgb, var(--accent) 18%, transparent);
  border-radius: 8px;
  padding: 7px 12px;
  letter-spacing: 0.2px;
}
.hero-code .ok { color: var(--deco-b); }
.hero-code .cursor {
  display: inline-block; width: 6px; height: 13px; margin-left: 6px;
  background: var(--accent); vertical-align: -2px; border-radius: 1px;
}

/* Hero 文字入场：eyebrow → h1 → sub → code 依次上浮 */
.hero-text > * { animation: hero-up 0.5s ease both; }
.hero-text .eyebrow { animation-delay: 0.05s; }
.hero-text h1 { animation-delay: 0.15s; }
.hero-text .hero-sub { animation-delay: 0.25s; }
.hero-text .hero-code { animation-delay: 0.35s; }
@keyframes hero-up {
  from { opacity: 0; transform: translateY(14px); }
  to { opacity: 1; transform: translateY(0); }
}
/* 月相呼吸光晕 */
.ha-moon { animation: moon-breathe 5s ease-in-out infinite; }
@keyframes moon-breathe {
  0%, 100% { filter: brightness(1); }
  50% { filter: brightness(1.08); }
}
@media (prefers-reduced-motion: reduce) {
  .hero-text > *, .ha-moon { animation: none; }
}

/* Hero 装饰画：月相 + 圆环层叠 */
.hero-art { position: relative; height: 300px; display: flex; align-items: center; justify-content: center; }
.ha-ring { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; opacity: 0.12; }
.ha-ring svg { width: 300px; height: 300px; }
.ha-dot {
  position: absolute; width: 5px; height: 5px; border-radius: 50%;
  background: var(--accent);
  left: 50%; top: 50%;
  transform: translate(38px, -132px);
  box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 50%, transparent);
}
.ha-moon { position: relative; display: flex; align-items: center; justify-content: center; cursor: pointer; }
.ha-moon svg { display: block; }
.ha-moon svg circle { opacity: 0.30; }
[data-theme="night"] .ha-moon svg circle { opacity: 0.10; }
.ha-moon::before {
  content: ""; position: absolute; inset: -40px;
  background: radial-gradient(circle, color-mix(in srgb, var(--accent) 7%, transparent), transparent 70%);
  border-radius: 50%;
}
.ha-scale {
  position: absolute; left: 50%; bottom: 6px; transform: translateX(-50%);
  display: flex; gap: 4px; align-items: flex-end; opacity: 0.5;
}
.ha-scale i { display: block; width: 2px; background: var(--text-dim); border-radius: 1px; }
.ha-tag {
  position: absolute; bottom: -16px; right: 6%;
  font-family: ui-monospace, Consolas, monospace; font-size: 12px;
  color: var(--text-dim); opacity: 0.8; letter-spacing: 1px;
  cursor: pointer;
}

.stage {
  position: relative;
  display: flex; flex-direction: column; align-items: center; gap: 34px;
  padding: 56px 60px 84px;
  z-index: 2;
}
.cf-geo {
  position: absolute; inset: 0; z-index: 0;
  display: flex; align-items: center; justify-content: center;
  pointer-events: none; opacity: 0.4;
}
.cf-geo svg { width: 640px; max-width: 92vw; }
.cf-word {
  position: absolute; z-index: 0; top: 44px;
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  font-size: clamp(68px, 11vw, 118px); font-weight: 800; letter-spacing: -6px;
  color: var(--text); opacity: 0.05; line-height: 1;
  user-select: none; pointer-events: none; white-space: nowrap;
}
.codeflow {
  position: relative; z-index: 1;
  display: flex; flex-direction: column; gap: 20px; align-items: center;
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
}
.cf-line {
  font-size: 12.5px; letter-spacing: 2px; color: var(--text-dim); opacity: 0.55;
}
.cf-line i { font-style: normal; color: var(--accent); opacity: 0.7; margin: 0 8px; }
.cf-line em { font-style: normal; color: var(--tint); opacity: 0.8; margin: 0 6px; }
.cf-b { margin-left: -64px; opacity: 0.4; }
.cf-c { margin-left: 56px; opacity: 0.5; }
.cf-d { margin-left: -32px; opacity: 0.45; }
.cf-scale {
  position: relative;
  width: 220px; height: 22px;
  display: flex; align-items: center;
  margin: 8px 0 4px; opacity: 0.6;
}
.cf-scale b {
  flex: 1; height: 10px; border-radius: 1px;
  background: var(--text-dim); display: block;
}
.cf-scale i {
  position: absolute; top: 50%;
  width: 2px; height: 18px; border-radius: 1px;
  background: var(--accent); display: block;
  transform: translate(-50%, -50%);
  box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 60%, transparent);
  transition: left 0.4s ease;
}
.cf-symbols {
  font-size: 17px; letter-spacing: 6px; color: var(--text-dim); opacity: 0.28;
}
.cf-symbols em { font-style: normal; color: var(--accent); opacity: 0.8; }
.cf-symbols2 { font-size: 13px; letter-spacing: 10px; opacity: 0.22; }
.cf-symbols2 em { color: var(--tint); opacity: 0.7; }
.stats-strip {
  position: relative; z-index: 1;
  display: flex; gap: 8px; flex-wrap: wrap; justify-content: center;
}
.stats-strip span {
  font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  font-size: 10.5px; color: var(--text-dim); letter-spacing: 0.5px;
  background: var(--surface); border: 1px solid var(--line);
  border-radius: 999px; padding: 3px 12px;
  transition: color 0.15s ease, border-color 0.15s ease;
}
.stats-strip span:hover { color: var(--accent); border-color: var(--accent); }

footer {
  padding: 8px 48px 30px;
  font-size: 11px; color: var(--text-dim); opacity: 0.7;
  position: relative; z-index: 2;
  display: flex; align-items: center; gap: 8px;
}
footer::before { content: ""; width: 14px; height: 2px; border-radius: 1px; background: var(--accent); }



@media (max-width: 860px) {
  .hero { grid-template-columns: 1fr; padding: 20px 28px; }
  .hero-art { height: 200px; }
  .cards { flex-wrap: wrap; padding: 0 28px 30px; }
  .topbar { padding: 20px 28px; }
}

/* 小屏（手机）细化 */
@media (max-width: 480px) {
  .topbar { padding: 16px 18px; }
  .hero { padding: 12px 18px 6px; gap: 22px; }
  .hero h1 { font-size: clamp(40px, 13vw, 56px); letter-spacing: -1.5px; }
  .hero-sub { font-size: 13px; margin-bottom: 20px; }
  .hero-code { font-size: 10.5px; padding: 6px 10px; }
  .eyebrow { font-size: 9px; letter-spacing: 3px; }
  .stage { padding: 34px 18px 54px; gap: 26px; }
  .stats-strip span { font-size: 9.5px; padding: 2px 10px; }
  footer { padding: 6px 18px 24px; flex-wrap: wrap; gap: 6px; }
  .wave { height: 90px; opacity: 0.05; }
}

/* ---- 满月形态（月圆时红→绿、冰蓝→淡青、夜间青黑） ---- */
html[data-moon="full"] {
  --accent: #2f9e63;
  --tint: #6fbdbd;
  --tint-soft: #e6f5f5;
}
html[data-moon="full"][data-theme="night"] {
  --accent: #4fd08a;
  --tint: #63c9c9;
  --tint-soft: #1e2f3a;
  --bg: #0e1724;
  --surface: #15202e;
  --line: #2a3848;
  --text-dim: #90a4b8;
}
html[data-moon="full"] .bm-logo {
  filter: hue-rotate(120deg);
}
</style>
</head>
<body>

<div class="deco glow-tl"></div>
<div class="deco glow-br"></div>

<div class="deco wave">
  <svg viewBox="0 0 1440 140" preserveAspectRatio="none">
    <defs>
      <linearGradient id="wg" x1="0" y1="0" x2="1" y2="0">
        <stop offset="0" stop-color="var(--accent)" stop-opacity="0"/>
        <stop offset="0.5" stop-color="var(--accent)"/>
        <stop offset="1" stop-color="var(--accent)" stop-opacity="0"/>
      </linearGradient>
    </defs>
    <path d="M0 90 Q180 30 360 70 T720 60 T1080 70 T1440 50 V0 H0 Z" fill="url(#wg)"/>
  </svg>
</div>

<div class="deco code-tex c1">timeline 42 · entries 7</div>
<div class="deco code-tex c2">revert d9f2ab</div>
<div class="deco code-tex c3">glb export ✓</div>
<div class="deco code-tex c4">tag: 建模</div>

<header class="topbar">
  <div class="brand-mini">
    <img class="bm-logo" src="data:image/png;base64,{favicon}" alt="logo">
    Keinestamp
  </div>
  <div class="topbar-right">
    <button id="theme-toggle" class="theme-btn" title="切换日/夜">
      <svg class="icon-sun" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><circle cx="8" cy="8" r="3.2"/><path d="M8 1.2v1.8M8 13v1.8M1.2 8h1.8M13 8h1.8M3.2 3.2l1.3 1.3M11.5 11.5l1.3 1.3M12.8 3.2l-1.3 1.3M4.5 11.5l-1.3 1.3"/></svg>
      <svg class="icon-moon" viewBox="0 0 16 16" fill="currentColor"><path d="M14 9.2A6.5 6.5 0 1 1 6.8 2a5.3 5.3 0 0 0 7.2 7.2z"/></svg>
    </button>
  </div>
</header>

<main class="hero">
  <div class="hero-text">
    <div class="eyebrow">Lore · Timeline <span class="dot">●</span></div>
    <h1>{site_name}</h1>
    <p class="hero-sub">Lore Progress Board</p>
    <code class="hero-code"><span style="color:var(--text-dim)">$</span> record h1st0ry0 <span class="ok"><svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:-1px" aria-hidden="true"><circle cx="8" cy="8" r="6.4"/><path d="M5.3 8.2l1.9 1.9 3.5-3.9"/></svg> 已记录</span><span class="cursor"></span></code>
  </div>
  <div class="hero-art">
    <div class="ha-ring">
      <svg viewBox="0 0 300 300" fill="none">
        <circle cx="150" cy="150" r="140" stroke="var(--deco-a)" stroke-width="1"/>
        <circle cx="150" cy="150" r="112" stroke="var(--deco-b)" stroke-width="1" stroke-dasharray="3 9"/>
        <path d="M10 150 Q150 30 290 150" stroke="var(--deco-c)" stroke-width="1"/>
        <path d="M60 260 Q150 140 260 60" stroke="var(--deco-a)" stroke-width="1" opacity="0.7"/>
      </svg>
    </div>
    <div class="ha-moon">
      <svg width="170" height="170" viewBox="0 0 100 100">
        <circle cx="50" cy="50" r="42" fill="var(--tint)"/>
        <path id="moonLight" fill="var(--accent)"/>
      </svg>
    </div>
    <div class="ha-dot"></div>
    <div class="ha-scale">
      <i style="height:4px"></i><i style="height:7px"></i><i style="height:10px"></i><i style="height:7px"></i><i style="height:4px"></i>
    </div>
    <div class="ha-tag"><span id="moonName">月相</span></div>
  </div>
</main>

<div class="stage">
  <div class="cf-geo">
    <svg viewBox="0 0 640 320" fill="none">
      <circle cx="320" cy="160" r="145" stroke="var(--deco-a)" stroke-width="1"/>
      <circle cx="320" cy="160" r="110" stroke="var(--deco-b)" stroke-width="1" stroke-dasharray="3 9"/>
      <path d="M60 160 Q320 40 580 160" stroke="var(--deco-c)" stroke-width="1"/>
      <path d="M130 240 Q320 120 500 70" stroke="var(--deco-a)" stroke-width="1" opacity="0.7"/>
      <circle cx="96" cy="88" r="3" fill="var(--accent)" opacity="0.5"/>
      <circle cx="556" cy="228" r="3" fill="var(--tint)" opacity="0.5"/>
    </svg>
  </div>
  <span class="cf-word">timeline</span>
  <div class="codeflow">
    <span class="cf-line cf-a">record <i>·</i> commit <i>·</i> push</span>
    <span class="cf-line cf-b">fn timeline <i>→</i> index.html</span>
    <span class="cf-line cf-c">diff <em>◉</em> milestone <em>★</em> tag</span>
    <span class="cf-scale"><b></b><b></b><b></b><b></b><b></b><b></b><b></b><b></b><b></b><b></b><b></b><i id="cfMark"></i></span>
    <span class="cf-symbols">{ } [ ] ( ) <em>:</em> <em>::</em> ⟶ ⇄</span>
    <span class="cf-symbols cf-symbols2">‹ › « » … <em>·</em> <em>·</em> <em>·</em></span>
    <span class="cf-line cf-d">blend <i>·</i> glb <i>·</i> web</span>
  </div>
  <div class="stats-strip">
    <span>entries 42</span><span>commits 128</span><span>glb 7</span><span>uptime 99.9%</span><span>tz +08:00</span>
  </div>
</div>

<footer>Keinestamp · Lore Progress Board · Lore v0.8.6 · © 2026</footer>

<script>
  (function () {
    var RAD = Math.PI / 180;
    function elongation(jd) {
      var T = (jd - 2451545.0) / 36525.0;
      var D = 297.8501921 + 445267.1114034 * T - 0.0018819 * T * T + 0.0000004 * T * T * T;
      var M = 357.5291092 + 35999.0502909 * T - 0.0001536 * T * T;
      var Mp = 134.9634114 + 477198.8676313 * T + 0.0089970 * T * T;
      var F = 93.2720993 + 483202.0175273 * T - 0.0034029 * T * T;
      var Lp = 218.3164477 + 481267.88123421 * T - 0.0015786 * T * T;
      Lp += 6.288774 * Math.sin(Mp * RAD);
      Lp += 1.274018 * Math.sin((2 * D - Mp) * RAD);
      Lp += 0.658309 * Math.sin(2 * D * RAD);
      Lp += 0.213616 * Math.sin(2 * Mp * RAD);
      Lp -= 0.185596 * Math.sin(M * RAD);
      Lp -= 0.114336 * Math.sin(2 * F * RAD);
      Lp += 0.058793 * Math.sin((2 * D - 2 * Mp) * RAD);
      Lp += 0.057212 * Math.sin((2 * D - M - Mp) * RAD);
      Lp += 0.053320 * Math.sin((2 * D + Mp) * RAD);
      Lp += 0.045877 * Math.sin((2 * D - M) * RAD);
      Lp += 0.041033 * Math.sin((M - Mp) * RAD);
      var L = 280.46646 + 36000.76983 * T + 0.0003032 * T * T;
      var e = (Lp - L) % 360;
      if (e < 0) e += 360;
      return e;
    }
    function moonInfo(date) {
      var jd = date.getTime() / 86400000 + 2440587.5;
      var elong = elongation(jd);
      var phase = elong / 360;
      var illum = (1 - Math.cos(elong * RAD)) / 2;
      return { phase: phase, illum: illum };
    }
    function moonName(p, i) {
      if (i < 0.02) return '新月';
      if (i < 0.40) return p < 0.5 ? '蛾眉月' : '残月';
      if (i <= 0.60) return p < 0.5 ? '上弦月' : '下弦月';
      if (i < 0.95) return p < 0.5 ? '盈凸月' : '亏凸月';
      return '满月';
    }
    function moonPath(p) {
      var waxing = p < 0.5;
      var k = Math.cos(2 * Math.PI * p);
      var lit = (1 + k) / 2;
      var small = lit < 0.5;
      var kx = Math.abs(2 * lit - 1) * 42;
      if (kx < 0.6) kx = 0;
      var s1 = waxing ? 1 : 0;
      var s2 = small ? s1 : 1 - s1;
      return 'M 50 8 A 42 42 0 0 ' + s1 + ' 50 92 A ' + kx.toFixed(3) + ' 42 0 0 ' + s2 + ' 50 8 Z';
    }
    var now = new Date();
    var day = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 12);
    var real = moonInfo(day);
    var overridePhase = null;
    var NAME_IDX = { '新月': 0, '蛾眉月': 1, '上弦月': 2, '盈凸月': 3, '满月': 4, '亏凸月': 5, '下弦月': 6, '残月': 7 };
    var realIdx = NAME_IDX[moonName(real.phase, real.illum)];

    function currentPhase() {
      return overridePhase === null ? real.phase : overridePhase / 8;
    }
    function currentIllum() {
      if (overridePhase === null) return real.illum;
      return (1 - Math.cos(2 * Math.PI * (overridePhase / 8))) / 2;
    }
    var faviconLink = document.querySelector('link[rel="icon"]');
    var originalFavicon = faviconLink ? faviconLink.href : null;
    var greenFavicon = 'data:image/png;base64,{greenfavicon}';
    function setGreenFavicon(enable) {
      if (!faviconLink || !originalFavicon) return;
      faviconLink.href = enable ? greenFavicon : originalFavicon;
    }
    function renderMoon() {
      var p = currentPhase();
      var i = currentIllum();
      document.getElementById('moonLight').setAttribute('d', moonPath(p));
      document.getElementById('moonName').textContent = moonName(p, i);
      var cfMark = document.getElementById('cfMark');
      if (cfMark) {
        cfMark.style.left = (6 + (p % 1) * 208) + 'px';
      }
      var full = overridePhase === 4 || (overridePhase === null && real.illum >= 0.95);
      if (full) {
        document.documentElement.dataset.moon = 'full';
        setGreenFavicon(true);
      } else {
        delete document.documentElement.dataset.moon;
        setGreenFavicon(false);
      }
    }
    // 月晕脉冲：柔光扩散 + 月体轻弹
    function moonPulse() {
      var moon = document.querySelector('.ha-moon');
      if (!moon) return;
      var h = document.createElement('div');
      h.className = 'moon-halo';
      moon.appendChild(h);
      setTimeout(function () { h.remove(); }, 950);
      var svg = moon.querySelector('svg');
      if (svg) {
        svg.classList.remove('moon-pop');
        void svg.offsetWidth;
        svg.classList.add('moon-pop');
      }
    }
    renderMoon();

    // 月相转盘：点击拨动月相，走满一圈回到真实月相
    document.querySelector('.ha-moon').addEventListener('click', function () {
      if (overridePhase === null) overridePhase = realIdx;
      overridePhase = (overridePhase + 1) % 8;
      moonPulse();
      renderMoon();
      if (overridePhase === realIdx) {
        setTimeout(function () {
          if (overridePhase === realIdx) { overridePhase = null; moonPulse(); renderMoon(); }
        }, 900);
      }
    });
    document.getElementById('moonName').addEventListener('click', function () {
      overridePhase = null;
      renderMoon();
    });

    // 浮游星尘：月夜里缓缓上浮的微光
    (function spawnMotes() {
      for (var i = 0; i < 14; i++) {
        var m = document.createElement('div');
        m.className = 'mote';
        var size = 2 + Math.random() * 2.5;
        m.style.width = size.toFixed(1) + 'px';
        m.style.height = size.toFixed(1) + 'px';
        m.style.left = (Math.random() * 100).toFixed(1) + 'vw';
        m.style.setProperty('--mo', (0.1 + Math.random() * 0.25).toFixed(2));
        m.style.setProperty('--mx', (Math.random() * 120 - 60).toFixed(0) + 'px');
        var dur = 16 + Math.random() * 18;
        m.style.animationDuration = dur.toFixed(1) + 's';
        m.style.animationDelay = (-Math.random() * dur).toFixed(1) + 's';
        document.body.appendChild(m);
      }
    })();

    // 低语：偶尔浮过一句没有来由的话
    var WHISPERS = ['岁在玄枵', '月亮记得', '此夜无事可录', '一切过去之事皆有存根', '潮汐与历史同期', '白泽未醒', '暂行记录，永不磨灭', '第四十一条记录已被封存'];
    function spawnWhisper() {
      var w = document.createElement('div');
      w.className = 'whisper';
      w.textContent = WHISPERS[Math.floor(Math.random() * WHISPERS.length)];
      w.style.left = (6 + Math.random() * 80) + 'vw';
      w.style.top = (10 + Math.random() * 72) + 'vh';
      document.body.appendChild(w);
      setTimeout(function () { w.remove(); }, 5600);
    }
    (function loopWhisper() {
      setTimeout(function () { spawnWhisper(); loopWhisper(); }, 13000 + Math.random() * 12000);
    })();

    // 视差 + 月亮注视：光晕反向游移，月亮朝鼠标微倾（走 --gx/--gy 变量，不与轻弹动画抢 transform）
    var g1 = document.querySelector('.glow-tl');
    var g2 = document.querySelector('.glow-br');
    var ring = document.querySelector('.ha-ring');
    var moonSvg = document.querySelector('.ha-moon svg');
    var raf = null;
    document.addEventListener('mousemove', function (ev) {
      if (raf) return;
      raf = requestAnimationFrame(function () {
        raf = null;
        var nx = ev.clientX / window.innerWidth - 0.5;
        var ny = ev.clientY / window.innerHeight - 0.5;
        if (g1) g1.style.transform = 'translate(' + (nx * 26).toFixed(1) + 'px,' + (ny * 26).toFixed(1) + 'px)';
        if (g2) g2.style.transform = 'translate(' + (nx * -20).toFixed(1) + 'px,' + (ny * -20).toFixed(1) + 'px)';
        if (ring) ring.style.transform = 'translate(' + (nx * 10).toFixed(1) + 'px,' + (ny * 10).toFixed(1) + 'px)';
        if (moonSvg) {
          moonSvg.style.setProperty('--gx', (nx * 9).toFixed(1) + 'px');
          moonSvg.style.setProperty('--gy', (ny * 9).toFixed(1) + 'px');
        }
      });
    });

    // 刻度呼吸的错拍
    var bars = document.querySelectorAll('.cf-scale b');
    Array.prototype.forEach.call(bars, function (b, i) {
      b.style.animationDelay = (i * 0.13).toFixed(2) + 's';
    });

    // 待机漂移：无人操作时「当前」标记在时间轴上自己游走
    setInterval(function () {
      if (overridePhase !== null) return;
      var cfMark = document.getElementById('cfMark');
      if (cfMark) cfMark.style.left = (4 + Math.random() * 210).toFixed(1) + 'px';
    }, 4200);

    // 给开控制台的人留的话
    try {
      console.log('%c:::::::::', 'color:#dc4a44;font-family:monospace;font-size:14px;letter-spacing:3px');
      console.log('%c月亮记得每一次记录。', 'color:#8a8f98;font-family:monospace');
    } catch (e) { /* ignore */ }
  })();
</script>
<script>
  document.getElementById('theme-toggle').addEventListener('click', function () {
    var next = document.documentElement.dataset.theme === 'night' ? 'day' : 'night';
    document.documentElement.dataset.theme = next;
    localStorage.setItem('keinestamp_web_theme', next);
  });
</script>

</body>
</html>"##;

pub fn sanitize_project_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect()
}

fn versions_path(web_root: &Path) -> PathBuf {
    web_root.join(".versions.json")
}

fn load_versions(web_root: &Path) -> HashMap<String, u64> {
    std::fs::read_to_string(versions_path(web_root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_versions(web_root: &Path, versions: &HashMap<String, u64>) {
    if let Ok(json) = serde_json::to_string(versions) {
        let path = versions_path(web_root);
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, &json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

fn manifests_root(web_root: &Path) -> PathBuf {
    web_root.join(".manifests")
}

fn manifest_path(web_root: &Path, project_name: &str) -> PathBuf {
    manifests_root(web_root).join(format!("{}.json", project_name))
}

fn load_manifest(web_root: &Path, project_name: &str) -> Option<Vec<crate::push::DeployFile>> {
    std::fs::read_to_string(manifest_path(web_root, project_name))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn save_manifest(web_root: &Path, project_name: &str, manifest: &[crate::push::DeployFile]) {
    if let Ok(json) = serde_json::to_string(manifest) {
        let dir = manifests_root(web_root);
        let _ = std::fs::create_dir_all(&dir);
        let path = manifest_path(web_root, project_name);
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, &json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

fn handle_manifest(request: tiny_http::Request, web_root: &Path, project_name: &str) -> Result<()> {
    match load_manifest(web_root, project_name) {
        Some(manifest) => {
            let body = serde_json::to_string(&manifest)?;
            let hdr = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
            request.respond(Response::from_string(body).with_header(hdr))?;
        }
        None => {
            let response = Response::from_string("no manifest").with_status_code(404);
            request.respond(response)?;
        }
    }
    Ok(())
}

fn projects_file(web_root: &Path) -> PathBuf {
    web_root
        .parent()
        .map(|p| p.join(".keinestamp-projects.json"))
        .unwrap_or_else(|| web_root.join(".keinestamp-projects.json"))
}

fn load_projects(web_root: &Path) -> (HashMap<String, String>, HashMap<String, String>) {
    let path = projects_file(web_root);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            log_error(
                "PROJECTS",
                &format!(
                    "failed to read {:?}: {} — starting with empty project registry",
                    path, e
                ),
            );
            return (HashMap::new(), HashMap::new());
        }
    };
    if content.trim().is_empty() {
        return (HashMap::new(), HashMap::new());
    }
    match serde_json::from_str::<serde_json::Value>(&content) {
        Ok(v) => {
            let tokens = v
                .get("tokens")
                .and_then(|x| serde_json::from_value(x.clone()).ok())
                .unwrap_or_default();
            let ids = v
                .get("ids")
                .and_then(|x| serde_json::from_value(x.clone()).ok())
                .unwrap_or_default();
            (tokens, ids)
        }
        Err(e) => {
            log_error(
                "PROJECTS",
                &format!(
                    "corrupt projects file {:?}: {} — starting with empty project registry",
                    path, e
                ),
            );
            (HashMap::new(), HashMap::new())
        }
    }
}

fn save_projects(web_root: &Path, tokens: &HashMap<String, String>, ids: &HashMap<String, String>) {
    if let Ok(json) =
        serde_json::to_string_pretty(&serde_json::json!({ "tokens": tokens, "ids": ids }))
    {
        let path = projects_file(web_root);
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, &json).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
            }
            if std::fs::rename(&tmp, &path).is_err() {
                log_error(
                    "PROJECTS",
                    &format!("failed to save projects file {:?}", path),
                );
            }
        } else {
            log_error(
                "PROJECTS",
                &format!("failed to write projects file {:?}", path),
            );
        }
    }
}

fn handle_version(
    request: tiny_http::Request,
    project_name: &str,
    web_versions: &RwLock<HashMap<String, u64>>,
) -> Result<()> {
    let count = web_versions.read().unwrap().get(project_name).copied();
    let body = match count {
        Some(n) => format!(r#"{{"count":{}}}"#, n),
        None => r#"{"count":null}"#.to_string(),
    };
    let hdr = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    let response = Response::from_string(body).with_header(hdr);
    request.respond(response)?;
    Ok(())
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                out.push(byte as char);
            } else {
                out.push('%');
                out.push_str(&hex);
            }
        } else if c == '+' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// 正确解码 URL 路径中的 UTF-8 百分号编码（用于静态文件路径）。
/// 例如 `%E5%B9%B4` → `年`。
fn percent_decode_path(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                decoded.push(b);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn handle_login_post(
    mut request: tiny_http::Request,
    project_name: &str,
    project_tokens: &RwLock<HashMap<String, String>>,
    rate_limiter: &RateLimiter,
) -> Result<()> {
    let ip = request_ip_trusted(&request);
    if !rate_limiter.check(&ip) {
        log_warn("LOGIN", &format!("{} rate limited", ip));
        let response = Response::from_string("429 Too Many Requests").with_status_code(429);
        request.respond(response)?;
        return Ok(());
    }
    let mut body = String::new();
    let read_start = Instant::now();
    let mut reader = request.as_reader().take(MAX_LOGIN_BODY);
    loop {
        if read_start.elapsed() > BODY_READ_TIMEOUT {
            log_warn("LOGIN", &format!("{} body read timeout", ip));
            let response = Response::from_string("request timeout").with_status_code(408);
            request.respond(response)?;
            return Ok(());
        }
        let mut buf = [0u8; 512];
        let n = reader.read(&mut buf).context("failed to read login body")?;
        if n == 0 {
            break;
        }
        body.push_str(&String::from_utf8_lossy(&buf[..n]));
        if body.len() >= MAX_LOGIN_BODY as usize {
            break;
        }
    }

    let token = body
        .split('&')
        .find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            if parts.next()? == "token" {
                Some(parts.next().unwrap_or(""))
            } else {
                None
            }
        })
        .unwrap_or("");
    let decoded = percent_decode(token);

    let is_valid = project_tokens
        .read()
        .unwrap()
        .get(project_name)
        .map_or(false, |t| t == &decoded);

    if is_valid {
        log_info(
            "LOGIN",
            &format!("{} project={} → success", ip, project_name),
        );
        rate_limiter.record_success(&ip);
        // 动态 Secure：HTTPS 来源（反代 X-Forwarded-Proto=https 或 TLS 直连）加 Secure，
        // HTTP 直连（局域网自用）不加——公网堵 301 前明文盲区，局域网保持登录体验
        let secure = if request_is_https(&request) {
            "; Secure"
        } else {
            ""
        };
        let set_cookie = format!(
            "keinestamp_token={}; Path=/{}/; HttpOnly; SameSite=Lax{}",
            decoded, project_name, secure
        );
        let redirect = Response::from_string("")
            .with_status_code(302)
            .with_header(
                Header::from_bytes(
                    &b"Location"[..],
                    format!("/{}/", project_name).as_bytes().to_vec(),
                )
                .unwrap(),
            )
            .with_header(Header::from_bytes(&b"Set-Cookie"[..], set_cookie.as_bytes()).unwrap());
        request.respond(redirect)?;
    } else {
        log_warn(
            "LOGIN",
            &format!("{} project={} → failed", ip, project_name),
        );
        rate_limiter.record_failure(&ip);
        let redirect = Response::from_string("").with_status_code(302).with_header(
            Header::from_bytes(
                &b"Location"[..],
                format!("/{}/login?error=1", project_name)
                    .as_bytes()
                    .to_vec(),
            )
            .unwrap(),
        );
        request.respond(redirect)?;
    }
    Ok(())
}

/// Generate a random hex token (32 bytes from OS CSPRNG)
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).unwrap_or_default();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Auto-generate `serve.toml` with random tokens when none exists
pub fn auto_generate_config(
    path: &Path,
    port: u16,
    host: &str,
    web_root: &Path,
) -> Result<ServeConfig> {
    let dt = generate_token();
    let canonical_root = std::fs::canonicalize(web_root).unwrap_or_else(|_| web_root.to_path_buf());
    let config_toml = format!(
        r#"host = "{}"
port = {}
deploy_token = "{}"
web_root = "{}"
# 备份服务器：true = serve 启动时拉起 loreserver（持久化 store，跟随 serve 生命周期）
enable_loreserver = false
"#,
        host,
        port,
        dt,
        canonical_root.to_string_lossy().replace('\\', "\\\\")
    );
    std::fs::write(&path, &config_toml)
        .with_context(|| format!("failed to write auto-generated serve config to {:?}", path))?;
    let config = load_config(path)?;
    let path_str = path.display().to_string();
    let root_str = canonical_root.display().to_string();
    log_info(
        "SETUP",
        &format!("First run — auto-generated serve.toml at {:?}", path_str),
    );
    log_info("SETUP", &format!("Deploy Token (for app): {}", dt));
    log_info(
        "SETUP",
        &format!("Web URL: http://{}:{}/<项目名>", host, port),
    );
    log_info(
        "SETUP",
        &format!("Deploy directory: {} (per project)", root_str),
    );
    log_info(
        "SETUP",
        "Access tokens are set per-project via GUI and stored on the server during deploy.",
    );
    Ok(config)
}

// --- Server start ---

// --- loreserver（备份服务器）拉起 ---

/// loreserver 配置目录 / store 持久化目录（程序所在目录下，便携部署）
pub fn loreserver_config_dir() -> std::path::PathBuf {
    crate::app_config::exe_dir().join("lore-server-config")
}

fn loreserver_store_dir() -> std::path::PathBuf {
    crate::app_config::exe_dir().join("lore-server-store")
}

fn loreserver_bin() -> Option<std::path::PathBuf> {
    let bin = crate::lore_backend::lore_bin();
    let dir = std::path::Path::new(&bin).parent()?;
    let name = if cfg!(target_os = "windows") {
        "loreserver.exe"
    } else {
        "loreserver"
    };
    let p = dir.join(name);
    p.exists().then_some(p)
}

/// 确保 local.toml 存在（持久化 store 路径；缺失时生成）
fn ensure_loreserver_config() -> Result<()> {
    let dir = loreserver_config_dir();
    std::fs::create_dir_all(&dir)?;
    let toml_path = dir.join("local.toml");
    if !toml_path.exists() {
        let store = loreserver_store_dir();
        let cfg = format!(
            "[immutable_store]\nmode = \"local\"\n[immutable_store.local]\npath = \"{}/immutable\"\n[mutable_store]\nmode = \"local\"\n[mutable_store.local]\npath = \"{}/mutable\"\n",
            store.to_string_lossy().replace('\\', "/"),
            store.to_string_lossy().replace('\\', "/")
        );
        std::fs::write(&toml_path, cfg)?;
    }
    Ok(())
}

/// 探测 loreserver 在线（HTTP health_check，41339；复用 lore_backend 探测逻辑）
pub fn loreserver_online() -> bool {
    crate::lore_backend::lore_server_health("127.0.0.1")
}

/// loreserver 启动输出解析：版本 / store 路径（immutable+mutable）/ 认证状态。
/// 输出来自 `Server version: x` 与 `Loaded config: Settings { ... }` 行。
fn parse_loreserver_info(output: &str) -> (String, String, String) {
    let version = output
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("Server version:")
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "未知".to_string());
    // Loaded config 里 store 路径形如 `LocalImmutableStoreSettings { ..., path: "C:/...", ... }`
    let mut stores: Vec<String> = Vec::new();
    for anchor in ["LocalImmutableStoreSettings", "LocalMutableStoreSettings"] {
        if let Some(start) = output.find(anchor) {
            let rest = &output[start..];
            if let Some(p) = rest.find("path: \"") {
                let v = &rest[p + 7..];
                if let Some(end) = v.find('"') {
                    if !v[..end].is_empty() {
                        stores.push(v[..end].to_string());
                    }
                }
            }
        }
    }
    let store = if stores.is_empty() {
        "默认（系统临时目录，重启丢失）".to_string()
    } else {
        stores.join(" | ")
    };
    let auth = if output.contains("auth: None") {
        "无认证，仅限可信局域网".to_string()
    } else {
        "已配置认证".to_string()
    };
    (version, store, auth)
}

/// 确保 loreserver 在线：探测 → 离线则拉起（--config 持久化）→ 轮询等待健康（~10s）。
/// 返回拉起的子进程（调用方负责退出时 kill）；已在线则 None。
/// 拉起后解析启动输出（版本/store 路径/认证）打印结构化日志；常驻读取线程转发后续输出防管道阻塞。
pub fn ensure_loreserver() -> Result<Option<std::process::Child>> {
    if loreserver_online() {
        log_info("LORESERVER", "already online");
        return Ok(None);
    }
    let bin = loreserver_bin().context("loreserver 未找到（Lore 安装目录）")?;
    ensure_loreserver_config()?;
    let cfg_dir = loreserver_config_dir();
    log_info("LORESERVER", &format!("starting {}", bin.display()));
    let mut child = crate::process_util::hide_console(std::process::Command::new(&bin))
        .arg("--config")
        .arg(&cfg_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("拉起 loreserver 失败: {}", e))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    // 读取线程：就绪前收集到共享缓冲（供解析），就绪后逐行转发（防管道写满阻塞）
    let startup_buf: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let ready: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    if let Some(mut so) = stdout {
        let buf = startup_buf.clone();
        let ready = ready.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut b = [0u8; 4096];
            loop {
                match so.read(&mut b) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let s = String::from_utf8_lossy(&b[..n]).into_owned();
                        if ready.load(Ordering::Relaxed) {
                            for line in s.lines() {
                                if !line.trim().is_empty() {
                                    log_info("LORESERVER", &format!("> {}", line.trim()));
                                }
                            }
                        } else if let Ok(mut g) = buf.lock() {
                            g.push_str(&s);
                        }
                    }
                }
            }
        });
    }
    if let Some(mut se) = stderr {
        let buf = startup_buf.clone();
        let ready = ready.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut b = [0u8; 4096];
            loop {
                match se.read(&mut b) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let s = String::from_utf8_lossy(&b[..n]).into_owned();
                        if ready.load(Ordering::Relaxed) {
                            for line in s.lines() {
                                if !line.trim().is_empty() {
                                    log_warn("LORESERVER", &format!("> {}", line.trim()));
                                }
                            }
                        } else if let Ok(mut g) = buf.lock() {
                            g.push_str(&s);
                        }
                    }
                }
            }
        });
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(300));
        if loreserver_online() {
            ready.store(true, Ordering::Relaxed);
            // 结构化信息输出（版本/store/认证）
            let startup_out = startup_buf.lock().map(|g| g.clone()).unwrap_or_default();
            let (version, store, auth) = parse_loreserver_info(&startup_out);
            log_info("LORESERVER", &format!("lore {}（{}）", version, auth));
            log_info("LORESERVER", &format!("store: {}", store));
            log_info("LORESERVER", "监听 41337 (gRPC/QUIC) + 41339 (HTTP)");
            return Ok(Some(child));
        }
        if let Ok(Some(status)) = child.try_wait() {
            anyhow::bail!("loreserver 启动后退出: {}", status);
        }
    }
    let _ = child.kill();
    anyhow::bail!("loreserver 启动超时（10s）")
}

pub fn start(config: ServeConfig) -> Result<()> {
    // 前置依赖检测：Lore v0.8.6 缺失/版本不符 → 拒绝启动
    crate::lore_backend::check_lore_env().context("Lore 前置依赖检查失败")?;
    let _ = TRUST_PROXY.set(config.trust_proxy);
    let _ = SITE_NAME.set(
        config
            .site_name
            .clone()
            .unwrap_or_else(|| "Keinestamp".to_string()),
    );
    // 拉起 loreserver（跟随 serve 生命周期：退出时 kill 本进程拉起的实例；外部已在线不杀）
    let mut loreserver_child: Option<std::process::Child> = None;
    if config.enable_loreserver {
        match ensure_loreserver() {
            Ok(c) => loreserver_child = c,
            Err(e) => log_warn("LORESERVER", &format!("failed to start: {}", e)),
        }
    }
    let addr = format!("{}:{}", config.host, config.port);
    let server = Server::http(&addr)
        .map_err(|e| anyhow::anyhow!("failed to start server on {}: {}", addr, e))?;

    let web_root = config.web_root.clone();
    let _ = WEB_ROOT.set(web_root.clone());
    let deploy_token = config.deploy_token.clone();

    let three_dir = web_root.join("_three");
    if let Err(e) = three_assets::ensure_threejs(&three_dir) {
        log_error(
            "STARTUP",
            &format!("failed to generate shared three.js: {}", e),
        );
    } else {
        log_info("STARTUP", "shared three.js assets ready");
    }

    log_info(
        "STARTUP",
        &format!("Keinestamp serve listening on http://{}", addr),
    );
    log_warn("STARTUP", "HTTP mode — data is sent in plaintext.");
    log_warn(
        "STARTUP",
        "For production, put behind Caddy/nginx with HTTPS.",
    );
    log_info(
        "STARTUP",
        &format!("  {} (deploy token)", web_root.display()),
    );

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let last_update = Arc::new(AtomicU64::new(now));
    let (tokens_loaded, ids_loaded) = load_projects(&web_root);
    let project_tokens: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(tokens_loaded));
    let project_ids: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(ids_loaded));
    let web_versions: Arc<RwLock<HashMap<String, u64>>> =
        Arc::new(RwLock::new(load_versions(&web_root)));
    let deploy_limiter = Arc::new(RateLimiter::new());
    let browser_limiter = Arc::new(RateLimiter::new());

    let active_requests = Arc::new(AtomicUsize::new(0));
    for request in server.incoming_requests() {
        if active_requests.load(Ordering::Relaxed) >= MAX_CONCURRENT_REQUESTS {
            let _ = request
                .respond(Response::from_string("503 Too Many Requests").with_status_code(503));
            continue;
        }
        active_requests.fetch_add(1, Ordering::Relaxed);
        let rl = deploy_limiter.clone();
        let brl = browser_limiter.clone();
        let dt = deploy_token.clone();
        let pt = project_tokens.clone();
        let pi = project_ids.clone();
        let wv = web_versions.clone();
        let wr = web_root.clone();
        let lu = last_update.clone();
        let active = active_requests.clone();
        let method = request.method().clone();
        let url = request.url().to_string();
        let req_ip = request_ip_trusted(&request);
        std::thread::spawn(move || {
            let _guard = SlotGuard(active);
            let _ = handle_request(
                request, url, method, req_ip, dt, pt, pi, wv, wr, lu, rl, brl,
            );
        });
    }

    // serve 退出：kill 拉起的 loreserver（跟随服务端生命周期）
    if let Some(mut child) = loreserver_child {
        log_info("LORESERVER", "stopping spawned loreserver");
        let _ = child.kill();
        let _ = child.wait();
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_request(
    request: tiny_http::Request,
    url: String,
    method: Method,
    req_ip: IpAddr,
    dt: Option<String>,
    pt: Arc<RwLock<HashMap<String, String>>>,
    pi: Arc<RwLock<HashMap<String, String>>>,
    wv: Arc<RwLock<HashMap<String, u64>>>,
    web_root: PathBuf,
    lu: Arc<AtomicU64>,
    rl: Arc<RateLimiter>,
    brl: Arc<RateLimiter>,
) -> Result<()> {
    // /health
    if url.split('?').next() == Some("/health") && method == Method::Get {
        let auth = check_deploy_auth(&request, dt.as_deref(), &rl);
        match auth {
            AuthResult::Authenticated => {
                if let Err(e) = handle_health(request, &lu) {
                    log_error("HEALTH", &format!("{} {}", req_ip, e));
                }
            }
            AuthResult::RateLimited => {
                log_warn("AUTH", &format!("{} rate limited on /health", req_ip));
                let _ = request
                    .respond(Response::from_string("429 Too Many Requests").with_status_code(429));
            }
            AuthResult::Unauthorized => {
                log_warn("AUTH", &format!("{} unauthorized on /health", req_ip));
                let _ = request.respond(unauthorized_response());
            }
        }
        return Ok(());
    }

    // /upload
    if url == "/upload" && method == Method::Post {
        let auth = check_deploy_auth(&request, dt.as_deref(), &rl);
        match auth {
            AuthResult::Authenticated => {
                if let Err(e) = handle_upload(request, &web_root, &lu, &pt, &pi, &wv) {
                    log_error("DEPLOY", &format!("{} {}", req_ip, e));
                }
            }
            AuthResult::RateLimited => {
                log_warn("AUTH", &format!("{} rate limited on /upload", req_ip));
                let _ = request
                    .respond(Response::from_string("429 Too Many Requests").with_status_code(429));
            }
            AuthResult::Unauthorized => {
                log_warn("AUTH", &format!("{} unauthorized on /upload", req_ip));
                let _ = request.respond(unauthorized_response());
            }
        }
        return Ok(());
    }

    // Root homepage
    if (url == "/" || url.is_empty()) && method == Method::Get {
        if let Err(e) = handle_index(request) {
            log_error("INDEX", &format!("{} {}", req_ip, e));
        }
        return Ok(());
    }

    // Browser routes
    let trimmed = url.trim_start_matches('/');
    let slash_pos = trimmed.find('/');
    let (project_name, rest) = match slash_pos {
        Some(pos) => (&trimmed[..pos], &trimmed[pos..]),
        None => (trimmed, "/"),
    };

    if project_name.is_empty()
        || !project_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        let _ = request.respond(Response::from_string("404 Not Found").with_status_code(404));
        return Ok(());
    }

    if !web_root.join(project_name).is_dir() {
        let _ = request.respond(Response::from_string("404 Not Found").with_status_code(404));
        return Ok(());
    }

    let route_path = if rest.is_empty() { "/" } else { rest };

    if route_path == "/rollback" && method == Method::Post {
        let auth = check_deploy_auth(&request, dt.as_deref(), &rl);
        match auth {
            AuthResult::Authenticated => {
                if let Err(e) = handle_rollback(request, &web_root, project_name) {
                    log_error("ROLLBACK", &format!("{} {}", req_ip, e));
                }
            }
            AuthResult::RateLimited => {
                log_warn("AUTH", &format!("{} rate limited on rollback", req_ip));
                let _ = request
                    .respond(Response::from_string("429 Too Many Requests").with_status_code(429));
            }
            AuthResult::Unauthorized => {
                log_warn("AUTH", &format!("{} unauthorized on rollback", req_ip));
                let _ = request.respond(unauthorized_response());
            }
        }
        return Ok(());
    }

    if route_path == "/version" && method == Method::Get {
        let auth = check_deploy_auth(&request, dt.as_deref(), &rl);
        match auth {
            AuthResult::Authenticated => {
                if let Err(e) = handle_version(request, project_name, &wv) {
                    log_error("VERSION", &format!("{} {}", req_ip, e));
                }
            }
            AuthResult::RateLimited => {
                log_warn("AUTH", &format!("{} rate limited on /version", req_ip));
                let _ = request
                    .respond(Response::from_string("429 Too Many Requests").with_status_code(429));
            }
            AuthResult::Unauthorized => {
                log_warn("AUTH", &format!("{} unauthorized on /version", req_ip));
                let _ = request.respond(unauthorized_response());
            }
        }
        return Ok(());
    }

    if route_path == "/manifest" && method == Method::Get {
        let auth = check_deploy_auth(&request, dt.as_deref(), &rl);
        match auth {
            AuthResult::Authenticated => {
                if let Err(e) = handle_manifest(request, &web_root, project_name) {
                    log_error("MANIFEST", &format!("{} {}", req_ip, e));
                }
            }
            AuthResult::RateLimited => {
                log_warn("AUTH", &format!("{} rate limited on /manifest", req_ip));
                let _ = request
                    .respond(Response::from_string("429 Too Many Requests").with_status_code(429));
            }
            AuthResult::Unauthorized => {
                log_warn("AUTH", &format!("{} unauthorized on /manifest", req_ip));
                let _ = request.respond(unauthorized_response());
            }
        }
        return Ok(());
    }

    if route_path == "/login" && method == Method::Get {
        if let Err(e) = serve_login_page(request, project_name) {
            log_error("LOGIN", &format!("{} serve_login_page: {}", req_ip, e));
        }
    } else if route_path == "/login" && method == Method::Post {
        if let Err(e) = handle_login_post(request, project_name, &pt, &brl) {
            log_error("LOGIN", &format!("{} handle_login_post: {}", req_ip, e));
        }
    } else {
        let auth = check_browser_static_auth(&request, &brl, project_name, &pt);
        match auth {
            AuthResult::Authenticated => {
                if let Err(e) = handle_static(request, &web_root, project_name) {
                    log_error("ACCESS", &format!("{} handle_static: {}", req_ip, e));
                }
            }
            AuthResult::RateLimited => {
                log_warn(
                    "AUTH",
                    &format!("{} rate limited on project '{}'", req_ip, project_name),
                );
                let _ = request
                    .respond(Response::from_string("429 Too Many Requests").with_status_code(429));
            }
            AuthResult::Unauthorized => {
                let is_browser = request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Accept"))
                    .map(|h| h.value.as_str().contains("text/html"))
                    .unwrap_or(false);
                if is_browser && method == Method::Get {
                    log_info(
                        "AUTH",
                        &format!(
                            "{} unauthorized → login page (project='{}')",
                            req_ip, project_name
                        ),
                    );
                    if let Err(e) = serve_login_page(request, project_name) {
                        log_error("LOGIN", &format!("{} serve_login_page: {}", req_ip, e));
                    }
                } else {
                    log_warn(
                        "AUTH",
                        &format!("{} unauthorized on project '{}'", req_ip, project_name),
                    );
                    let _ = request.respond(unauthorized_response());
                }
            }
        }
    }

    Ok(())
}

// --- Request handlers ---

fn handle_health(request: tiny_http::Request, last_update: &AtomicU64) -> Result<()> {
    let ip = request_ip_trusted(&request);
    let ts = last_update.load(Ordering::Relaxed);
    let body = format!(r#"{{"status":"ok","lastUpdate":{}}}"#, ts);
    let hdr = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    let response = Response::from_string(body).with_header(hdr);
    log_info("HEALTH", &format!("{} → 200", ip));
    request.respond(response)?;
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn apply_patch_dir(patch: &Path, target: &Path) -> Result<()> {
    for entry in std::fs::read_dir(patch)? {
        let entry = entry?;
        let src = entry.path();
        let dst = target.join(entry.file_name());
        if src.is_dir() {
            apply_patch_dir(&src, &dst)?;
        } else if src.is_file() {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

fn collect_all_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_all_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn remove_empty_dirs(dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            remove_empty_dirs(&path)?;
            let _ = std::fs::remove_dir(&path);
        }
    }
    Ok(())
}

fn apply_manifest_deletions(dir: &Path, manifest: &[crate::push::DeployFile]) -> Result<()> {
    let keep: HashSet<&str> = manifest.iter().map(|f| f.path.as_str()).collect();
    let mut files = Vec::new();
    collect_all_files(dir, &mut files)?;
    for path in files {
        let Ok(rel) = path.strip_prefix(dir) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if !keep.contains(rel_str.as_str()) {
            let _ = std::fs::remove_file(&path);
        }
    }
    remove_empty_dirs(dir)?;
    Ok(())
}

fn handle_upload(
    mut request: tiny_http::Request,
    web_root: &Path,
    last_update: &AtomicU64,
    project_tokens: &RwLock<HashMap<String, String>>,
    project_ids: &RwLock<HashMap<String, String>>,
    web_versions: &RwLock<HashMap<String, u64>>,
) -> Result<()> {
    let ip = request_ip_trusted(&request);
    let mut body = Vec::new();
    let read_start = Instant::now();
    let mut reader = request.as_reader().take(MAX_UPLOAD_SIZE as u64);
    let mut chunk = [0u8; 8192];
    loop {
        if read_start.elapsed() > BODY_READ_TIMEOUT {
            log_warn("DEPLOY", &format!("{} body read timeout", ip));
            let response = Response::from_string("request timeout").with_status_code(408);
            request.respond(response)?;
            return Ok(());
        }
        let n = reader
            .read(&mut chunk)
            .context("failed to read upload body")?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
        if body.len() >= MAX_UPLOAD_SIZE {
            break;
        }
    }
    let size = body.len();

    if size >= MAX_UPLOAD_SIZE {
        log_warn(
            "DEPLOY",
            &format!("{} payload too large ({} bytes)", ip, size),
        );
        let response = Response::from_string("payload too large").with_status_code(413);
        request.respond(response)?;
        return Ok(());
    }

    if body.is_empty() {
        log_warn("DEPLOY", &format!("{} empty body", ip));
        let response = Response::from_string("empty body").with_status_code(400);
        request.respond(response)?;
        return Ok(());
    }

    let project_name_raw = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Project-Name"))
        .map(|h| h.value.as_str())
        .filter(|v| !v.is_empty())
        .unwrap_or("unknown");
    let project_name = sanitize_project_name(project_name_raw);

    if project_name.is_empty() {
        log_warn("DEPLOY", &format!("{} invalid project name", ip));
        let response = Response::from_string("invalid project name").with_status_code(400);
        request.respond(response)?;
        return Ok(());
    }

    if project_name.starts_with('_') {
        log_warn(
            "DEPLOY",
            &format!("{} reserved project name '{}'", ip, project_name),
        );
        let response = Response::from_string("reserved project name").with_status_code(400);
        request.respond(response)?;
        return Ok(());
    }

    // Conflict check: verify project_id matches
    let project_id = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Project-Id"))
        .map(|h| h.value.as_str())
        .unwrap_or("");
    {
        let ids = project_ids.read().unwrap();
        if let Some(stored_id) = ids.get(&project_name) {
            if stored_id != project_id {
                log_warn(
                    "DEPLOY",
                    &format!(
                        "{} project='{}' conflict: project_id mismatch",
                        ip, project_name
                    ),
                );
                let response = Response::from_string("409 Conflict: project_id mismatch")
                    .with_status_code(409);
                request.respond(response)?;
                return Ok(());
            }
        } else if web_root.join(&project_name).is_dir() {
            log_warn(
                "DEPLOY",
                &format!(
                    "{} project='{}' exists but has no recorded identity — refusing takeover",
                    ip, project_name
                ),
            );
            let response = Response::from_string("403 Forbidden: existing project has no identity")
                .with_status_code(403);
            request.respond(response)?;
            return Ok(());
        }
    }
    if !project_id.is_empty() {
        project_ids
            .write()
            .unwrap()
            .insert(project_name.clone(), project_id.to_string());
    }

    if let Some(access_token) = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Access-Token"))
        .map(|h| h.value.as_str())
        .filter(|v| !v.is_empty())
    {
        project_tokens
            .write()
            .unwrap()
            .insert(project_name.clone(), access_token.to_string());
        log_info(
            "DEPLOY",
            &format!("{} project={} stored access_token", ip, project_name),
        );
    }

    {
        let tokens = project_tokens.read().unwrap().clone();
        let ids = project_ids.read().unwrap().clone();
        save_projects(&web_root, &tokens, &ids);
    }

    log_info(
        "DEPLOY",
        &format!("{} project={} size={} bytes", ip, project_name, size),
    );

    // Deploy into web_root/<project_name>/ for per-project isolation
    let deploy_dir = web_root.join(&project_name);
    let web_exists = deploy_dir.exists();

    let tmp_parent = deploy_dir.parent().unwrap_or(Path::new("."));
    let seq = UPLOAD_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_dir = tmp_parent.join(format!(".keinestamp-tmp.{}.{}", std::process::id(), seq));
    let bak_dir = tmp_parent.join(format!(".keinestamp-bak.{}.{}", std::process::id(), seq));

    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).context("failed to create temp dir")?;

    let cursor = std::io::Cursor::new(&body);
    let mut archive = match zip::ZipArchive::new(cursor) {
        Ok(a) => a,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            log_warn("DEPLOY", &format!("{} invalid zip: {}", ip, e));
            let msg = format!("invalid zip: {}", e);
            let response = Response::from_string(msg).with_status_code(400);
            request.respond(response)?;
            return Ok(());
        }
    };

    let mut extracted_total: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let entry_path = file.mangled_name();
        let dest = tmp_dir.join(&entry_path);

        let canonical_tmp = tmp_dir
            .canonicalize()
            .unwrap_or_else(|_| tmp_dir.to_path_buf());
        if let Some(parent) = dest.parent() {
            if let Ok(canonical_parent) = parent.canonicalize() {
                if !canonical_parent.starts_with(&canonical_tmp) {
                    continue;
                }
            }
        }

        if file.is_dir() {
            std::fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&dest)?;
            let copied = std::io::copy(&mut file, &mut out).map_err(|e| {
                let _ = std::fs::remove_dir_all(&tmp_dir);
                anyhow::anyhow!("extract error at entry {}: {}", i, e)
            })?;
            extracted_total += copied;
            if extracted_total > MAX_EXTRACT_SIZE {
                let _ = std::fs::remove_dir_all(&tmp_dir);
                log_warn(
                    "DEPLOY",
                    &format!(
                        "{} extraction exceeds limit ({} bytes)",
                        ip, extracted_total
                    ),
                );
                let response =
                    Response::from_string("payload expands beyond limit").with_status_code(413);
                request.respond(response)?;
                return Ok(());
            }
        }
    }

    // 增量部署：zip 内若带 `.keinestamp-manifest.json`，则按清单合并；否则全量替换
    let incremental_manifest: Option<Vec<crate::push::DeployFile>> = {
        let manifest_file = tmp_dir.join(crate::push::MANIFEST_ENTRY);
        if manifest_file.is_file() {
            let parsed = std::fs::read_to_string(&manifest_file)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());
            let _ = std::fs::remove_file(&manifest_file);
            parsed
        } else {
            None
        }
    };

    if web_exists {
        let _ = std::fs::remove_dir_all(&bak_dir);
        if let Err(e) = std::fs::rename(&deploy_dir, &bak_dir) {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            log_error("DEPLOY", &format!("{} {}", ip, e));
            let response = Response::from_string("deploy failed").with_status_code(500);
            request.respond(response)?;
            return Ok(());
        }
    }

    if let Some(manifest) = incremental_manifest {
        // 增量：复制旧目录 → 应用 changed 文件 → 按新清单删除多余文件
        if let Err(e) = std::fs::create_dir_all(&deploy_dir) {
            if web_exists {
                let _ = std::fs::rename(&bak_dir, &deploy_dir);
            }
            let _ = std::fs::remove_dir_all(&tmp_dir);
            log_error("DEPLOY", &format!("{} {}", ip, e));
            let response = Response::from_string("deploy failed").with_status_code(500);
            request.respond(response)?;
            return Ok(());
        }
        let apply_result = (|| -> Result<()> {
            if web_exists {
                copy_dir_all(&bak_dir, &deploy_dir)?;
            }
            apply_patch_dir(&tmp_dir, &deploy_dir)?;
            apply_manifest_deletions(&deploy_dir, &manifest)?;
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&tmp_dir);
        if let Err(e) = apply_result {
            let _ = std::fs::remove_dir_all(&deploy_dir);
            if web_exists {
                let _ = std::fs::rename(&bak_dir, &deploy_dir);
            }
            log_error("DEPLOY", &format!("{} {}", ip, e));
            let response = Response::from_string("deploy failed").with_status_code(500);
            request.respond(response)?;
            return Ok(());
        }
        save_manifest(web_root, &project_name, &manifest);
    } else {
        if let Err(e) = std::fs::rename(&tmp_dir, &deploy_dir) {
            if web_exists {
                let _ = std::fs::rename(&bak_dir, &deploy_dir);
            }
            let _ = std::fs::remove_dir_all(&tmp_dir);
            log_error("DEPLOY", &format!("{} {}", ip, e));
            let response = Response::from_string("deploy failed").with_status_code(500);
            request.respond(response)?;
            return Ok(());
        }
        // 全量上传没有新清单：清掉旧清单，避免后续增量客户端基于过期状态判断
        let _ = std::fs::remove_file(manifest_path(web_root, &project_name));
    }

    if web_exists {
        // 部署历史：把旧版本保留到 .history/<project>/<timestamp>，支持后续回滚
        let history_root = web_root.join(".history").join(&project_name);
        if let Ok(()) = std::fs::create_dir_all(&history_root) {
            let ts = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let dest = history_root.join(ts.to_string());
            let _ = std::fs::rename(&bak_dir, &dest);
        } else {
            let _ = std::fs::remove_dir_all(&bak_dir);
        }
    } else {
        let _ = std::fs::remove_dir_all(&bak_dir);
    }

    last_update.store(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        Ordering::Relaxed,
    );

    log_info(
        "DEPLOY",
        &format!("{} project={} → 200 /{}", ip, project_name, project_name),
    );

    let client_count = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Web-Count"))
        .and_then(|h| h.value.as_str().parse::<u64>().ok());
    if let Some(count) = client_count {
        web_versions
            .write()
            .unwrap()
            .insert(project_name.clone(), count);
        let snapshot = web_versions.read().unwrap().clone();
        save_versions(web_root, &snapshot);
    }

    let body = format!(r#"{{"status":"ok","prefix":"/{}"}}"#, project_name);
    let hdr = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    let response = Response::from_string(body).with_header(hdr);
    request.respond(response)?;
    Ok(())
}

fn handle_rollback(request: tiny_http::Request, web_root: &Path, project_name: &str) -> Result<()> {
    let history_root = web_root.join(".history").join(project_name);
    let mut snapshots: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&history_root) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                snapshots.push(p);
            }
        }
    }
    snapshots.sort();

    let Some(latest) = snapshots.last().cloned() else {
        let response = Response::from_string("no history").with_status_code(404);
        request.respond(response)?;
        return Ok(());
    };

    let deploy_dir = web_root.join(project_name);
    if deploy_dir.exists() {
        std::fs::remove_dir_all(&deploy_dir)?;
    }
    std::fs::rename(&latest, &deploy_dir)?;
    // 回滚后旧清单不再准确，删除以强制下一次全量部署
    let _ = std::fs::remove_file(manifest_path(web_root, project_name));

    let body = format!(r#"{{"status":"ok","rolled_back":"{}"}}"#, project_name);
    let hdr = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    let response = Response::from_string(body).with_header(hdr);
    request.respond(response)?;
    Ok(())
}

pub(crate) fn mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("glb") => "model/gltf-binary",
        Some("gltf") => "model/gltf+json",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}

fn handle_static(request: tiny_http::Request, web_root: &Path, project_name: &str) -> Result<()> {
    let ip = request_ip_trusted(&request);
    let method = request.method().to_string();
    let raw_path = request.url().to_string();
    let path_no_query = raw_path.split('?').next().unwrap_or(&raw_path).to_string();
    let prefix_path = format!("/{}", project_name);
    let without_prefix = path_no_query
        .strip_prefix(&prefix_path)
        .unwrap_or(&path_no_query);
    let clean_path = if without_prefix == "/" || without_prefix.is_empty() {
        "index.html".to_string()
    } else {
        percent_decode_path(without_prefix.trim_start_matches('/'))
    };

    let access_msg = format!("{} {} {} → ", ip, method, raw_path);

    if clean_path.starts_with('.') || clean_path.contains("/.") {
        log_warn("ACCESS", &format!("{}404 (dotfile)", access_msg));
        let response = Response::from_string("404 Not Found").with_status_code(404);
        request.respond(response)?;
        return Ok(());
    }

    let serve_dir = web_root.join(project_name);
    let shared_three = web_root.join("_three");

    let (file_path, canonical_base) = if let Some(three_rel) = clean_path.strip_prefix("three/") {
        (shared_three.join(three_rel), shared_three)
    } else {
        (serve_dir.join(&clean_path), serve_dir.clone())
    };

    let canonical_web_root = canonical_base
        .canonicalize()
        .unwrap_or_else(|_| canonical_base.to_path_buf());
    if file_path.exists() {
        if let Ok(canonical_file) = file_path.canonicalize() {
            if !canonical_file.starts_with(&canonical_web_root) {
                log_warn("ACCESS", &format!("{}403", access_msg));
                let response = Response::from_string("403 Forbidden").with_status_code(403);
                request.respond(response)?;
                return Ok(());
            }
        }
    }

    if file_path.exists() && file_path.is_file() {
        match std::fs::read(&file_path) {
            Ok(content) => {
                let mime = mime_type(&file_path);
                let hdr = Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
                let response = Response::from_data(content).with_header(hdr);
                log_info("ACCESS", &format!("{}200 ({})", access_msg, mime));
                request.respond(response)?;
            }
            Err(e) => {
                log_error("ACCESS", &format!("{}500: {}", access_msg, e));
                let response =
                    Response::from_string("500 Internal Server Error").with_status_code(500);
                request.respond(response)?;
            }
        }
    } else {
        let fallback = serve_dir.join("index.html");
        if fallback.exists() {
            match std::fs::read(&fallback) {
                Ok(content) => {
                    let mime = mime_type(&fallback);
                    let hdr = Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
                    let response = Response::from_data(content).with_header(hdr);
                    log_info(
                        "ACCESS",
                        &format!("{}200 → index.html ({})", access_msg, mime),
                    );
                    request.respond(response)?;
                }
                Err(_) => {
                    log_info(
                        "ACCESS",
                        &format!("{}404 (fallback read error)", access_msg),
                    );
                    let response = Response::from_string("404 Not Found").with_status_code(404);
                    request.respond(response)?;
                }
            }
        } else {
            log_info("ACCESS", &format!("{}404", access_msg));
            let response = Response::from_string("404 Not Found").with_status_code(404);
            request.respond(response)?;
        }
    }

    Ok(())
}

pub fn serve_local(web_root: &Path, port_tx: std::sync::mpsc::Sender<u16>) -> Result<()> {
    let canonical_web = web_root
        .canonicalize()
        .unwrap_or_else(|_| web_root.to_path_buf());
    for port in 47799u16..47810 {
        let addr = format!("127.0.0.1:{}", port);
        let Ok(server) = Server::http(&addr) else {
            continue;
        };
        let _ = port_tx.send(port);
        for request in server.incoming_requests() {
            let url = request.url().to_string();
            let path_only = url.split('?').next().unwrap_or(&url);
            let clean_path = if path_only == "/" || path_only.is_empty() {
                "index.html".to_string()
            } else {
                percent_decode_path(path_only.trim_start_matches('/'))
            };
            if clean_path.starts_with('.') || clean_path.contains("/.") {
                let _ = request.respond(Response::from_string("404").with_status_code(404));
                continue;
            }
            let file_path = web_root.join(&clean_path);
            if file_path.exists() && file_path.is_file() {
                if let Ok(canonical_file) = file_path.canonicalize() {
                    if !canonical_file.starts_with(&canonical_web) {
                        let _ = request.respond(Response::from_string("403").with_status_code(403));
                        continue;
                    }
                }
                match std::fs::read(&file_path) {
                    Ok(content) => {
                        let mime = mime_type(&file_path);
                        let hdr =
                            Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
                        let response = Response::from_data(content).with_header(hdr);
                        let _ = request.respond(response);
                    }
                    Err(_) => {
                        let _ = request.respond(Response::from_string("500").with_status_code(500));
                    }
                }
            } else {
                let fallback = web_root.join("index.html");
                if fallback.exists() && fallback.is_file() {
                    match std::fs::read(&fallback) {
                        Ok(content) => {
                            let mime = mime_type(&fallback);
                            let hdr =
                                Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
                            let response = Response::from_data(content).with_header(hdr);
                            let _ = request.respond(response);
                        }
                        Err(_) => {
                            let _ =
                                request.respond(Response::from_string("500").with_status_code(500));
                        }
                    }
                } else {
                    let _ = request.respond(Response::from_string("404").with_status_code(404));
                }
            }
        }
        return Ok(());
    }
    anyhow::bail!("no available port for local server")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_loreserver_info_extracts_version_store_auth() {
        let out = r#"Server version: 0.8.6+373
Using environment: local
Loaded config: Settings { environment: None, immutable_store: ImmutableStoreSettings { composite: None, local: Some(LocalImmutableStoreSettings { compaction_delay: None, eviction_delay: None, flush_delay_seconds: 10, path: "C:/Users/x/AppData/Roaming/Keinestamp/lore-server-store/immutable", max_capacity: None, max_size: None }), mode: "local", remote: None }, mutable_store: MutableStoreSettings { local: Some(LocalMutableStoreSettings { flush_delay_seconds: 10, path: "C:/Users/x/AppData/Roaming/Keinestamp/lore-server-store/mutable" }), mode: "local", remote: None }, server: ServerSettings { auth: None, grpc: Some(GrpcSettings { port: 41337 }) } }"#;
        let (version, store, auth) = parse_loreserver_info(out);
        assert_eq!(version, "0.8.6+373");
        assert_eq!(
            store,
            "C:/Users/x/AppData/Roaming/Keinestamp/lore-server-store/immutable | C:/Users/x/AppData/Roaming/Keinestamp/lore-server-store/mutable"
        );
        assert!(auth.contains("无认证"));
    }

    #[test]
    fn percent_decode_path_decodes_utf8_chinese() {
        assert_eq!(
            percent_decode_path("previews_image/%E5%B9%B4.png"),
            "previews_image/年.png"
        );
        assert_eq!(percent_decode_path("a%20b.png"), "a b.png");
        assert_eq!(percent_decode_path("plain.png"), "plain.png");
    }

    #[test]
    fn parse_loreserver_info_tolerates_minimal_output() {
        let (version, store, auth) = parse_loreserver_info("nothing here");
        assert_eq!(version, "未知");
        assert!(store.contains("默认"));
        assert_eq!(auth, "已配置认证");
    }

    #[test]
    fn projects_roundtrip_persists_tokens_and_ids() {
        let dir = std::env::temp_dir().join(format!("ks_proj_test_{}", std::process::id()));
        let web_root = dir.join(".keinestamp").join("web");
        std::fs::create_dir_all(&web_root).unwrap();
        let mut tokens = HashMap::new();
        tokens.insert("site-a".to_string(), "tok123".to_string());
        let mut ids = HashMap::new();
        ids.insert("site-a".to_string(), "id456".to_string());
        save_projects(&web_root, &tokens, &ids);
        let (t2, i2) = load_projects(&web_root);
        assert_eq!(t2.get("site-a").map(String::as_str), Some("tok123"));
        assert_eq!(i2.get("site-a").map(String::as_str), Some("id456"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn projects_load_tolerates_missing_and_corrupt_files() {
        let dir = std::env::temp_dir().join(format!("ks_proj_bad_{}", std::process::id()));
        let web_root = dir.join(".keinestamp").join("web");
        std::fs::create_dir_all(&web_root).unwrap();
        let (t, i) = load_projects(&web_root);
        assert!(t.is_empty() && i.is_empty());
        let p = projects_file(&web_root);
        std::fs::write(&p, "{ not json").unwrap();
        let (t2, i2) = load_projects(&web_root);
        assert!(t2.is_empty() && i2.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitize_project_name_filters_unicode() {
        assert_eq!(sanitize_project_name("33_芙姆米_新1皮"), "33__1");
        assert_eq!(sanitize_project_name("芙姆米"), "");
        assert_eq!(sanitize_project_name("My-Proj_2"), "My-Proj_2");
        assert_eq!(sanitize_project_name(&"a".repeat(80)), "a".repeat(64));
    }

    #[test]
    fn login_page_has_theme_and_project() {
        let html = login_page_html("my_proj");
        assert!(html.contains("data-theme"));
        assert!(html.contains("keinestamp_web_theme"));
        assert!(html.contains("my_proj"));
        assert!(html.contains("--accent"));
        assert!(!html.contains("<svg"), "web logo should be removed");
        assert!(html.contains("输入访问令牌以查看时间线"));
    }

    #[test]
    fn manifest_roundtrip_persists_deploy_files() {
        let dir = std::env::temp_dir().join(format!("ks_manifest_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let web_root = dir.join("web");
        std::fs::create_dir_all(&web_root).unwrap();
        let manifest = vec![
            crate::push::DeployFile {
                path: "index.html".to_string(),
                size: 10,
                hash: "abc".to_string(),
            },
            crate::push::DeployFile {
                path: "previews_glb/a.glb".to_string(),
                size: 20,
                hash: "def".to_string(),
            },
        ];
        save_manifest(&web_root, "proj", &manifest);
        let loaded = load_manifest(&web_root, "proj").unwrap();
        assert_eq!(loaded, manifest);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn incremental_merge_applies_patch_and_deletes_removed_files() {
        let dir = std::env::temp_dir().join(format!("ks_incr_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let deploy = dir.join("deploy");
        let patch = dir.join("patch");
        std::fs::create_dir_all(&deploy).unwrap();
        std::fs::create_dir_all(&patch).unwrap();
        std::fs::write(deploy.join("old.txt"), "old").unwrap();
        std::fs::write(deploy.join("remove.txt"), "remove").unwrap();
        std::fs::create_dir_all(deploy.join("emptydir")).unwrap();
        std::fs::write(patch.join("new.txt"), "new").unwrap();
        std::fs::create_dir_all(patch.join("sub")).unwrap();
        std::fs::write(patch.join("sub/nested.txt"), "nested").unwrap();

        apply_patch_dir(&patch, &deploy).unwrap();
        let manifest = vec![
            crate::push::DeployFile {
                path: "old.txt".to_string(),
                size: 3,
                hash: "old".to_string(),
            },
            crate::push::DeployFile {
                path: "new.txt".to_string(),
                size: 3,
                hash: "new".to_string(),
            },
            crate::push::DeployFile {
                path: "sub/nested.txt".to_string(),
                size: 6,
                hash: "nested".to_string(),
            },
        ];
        apply_manifest_deletions(&deploy, &manifest).unwrap();

        assert!(deploy.join("old.txt").exists());
        assert!(deploy.join("new.txt").exists());
        assert!(deploy.join("sub/nested.txt").exists());
        assert!(!deploy.join("remove.txt").exists());
        assert!(
            !deploy.join("emptydir").exists(),
            "empty dirs should be cleaned"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_page_has_brand_and_features() {
        let html = index_page_html();
        assert!(html.contains("Keinestamp"));
        assert!(html.contains("Lore Progress Board"));
        assert!(html.contains("data-theme"));
        assert!(html.contains("keinestamp_web_theme"));
        assert!(html.contains("data:image/png;base64,"), "favicon embedded");
        assert!(html.contains("eyebrow"), "eyebrow label present");
        assert!(html.contains("hero-code"), "code block present");
        assert!(html.contains("ha-moon"), "moon art present");
        assert!(html.contains("ha-scale"), "progress scale present");
        assert!(html.contains("ha-ring"), "ring decoration present");
        assert!(html.contains("ha-tag"), "moon phase label present");
        assert!(html.contains("moonLight"), "client-side moon path element");
        assert!(html.contains("elongation"), "client-side moon algorithm");
        assert!(html.contains("moonName"), "client-side moon name element");
        assert!(html.contains("theme-toggle"), "theme toggle button present");
    }

    #[test]
    fn log_ts_formats_current_date() {
        let ts = log_ts();
        assert!(
            ts.len() == 19 && ts.chars().nth(4) == Some('-') && ts.chars().nth(10) == Some(' '),
            "unexpected format: {}",
            ts
        );
        let year = ts[..4].parse::<i32>().unwrap();
        assert!(year >= 2025, "year out of range: {}", ts);
    }

    #[test]
    fn versions_roundtrip() {
        let dir = std::env::temp_dir().join(format!("keinestamp_versions_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut map: HashMap<String, u64> = HashMap::new();
        map.insert("proj_a".to_string(), 3);
        map.insert("proj_b".to_string(), 0);
        save_versions(&dir, &map);
        let loaded = load_versions(&dir);
        assert_eq!(loaded.get("proj_a"), Some(&3));
        assert_eq!(loaded.get("proj_b"), Some(&0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn versions_load_missing_is_empty() {
        let dir = std::env::temp_dir().join(format!(
            "keinestamp_versions_missing_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load_versions(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_load_config() -> Result<()> {
        let dir =
            std::env::temp_dir().join(format!("keinestamp_test_serve_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let config_path = dir.join("serve.toml");
        std::fs::write(
            &config_path,
            r#"
host = "127.0.0.1"
port = 8888
deploy_token = "abc123"
web_root = "web-root"
"#,
        )?;
        let config = load_config(&config_path)?;
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8888);
        assert_eq!(config.deploy_token.as_deref(), Some("abc123"));
        // 相对 web_root → 解析为程序所在目录下（便携部署）
        let expected = crate::app_config::exe_dir().join("web-root");
        assert_eq!(config.web_root, expected);
        // 绝对 web_root → 直接用
        let abs_path = dir.join("abs-web");
        let abs_str = abs_path.to_string_lossy().replace('\\', "\\\\");
        std::fs::write(
            &config_path,
            format!(
                "host = \"127.0.0.1\"\nport = 8888\ndeploy_token = \"abc123\"\nweb_root = \"{}\"\n",
                abs_str
            ),
        )?;
        let config = load_config(&config_path)?;
        assert_eq!(config.web_root, abs_path);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_load_config_no_token_fails() {
        let dir = std::env::temp_dir().join(format!(
            "keinestamp_test_serve_notok_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("serve.toml");
        std::fs::write(
            &config_path,
            r#"
web_root = "/tmp/web"
"#,
        )
        .unwrap();
        let result = load_config(&config_path);
        assert!(result.is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_rate_limiter() {
        let rl = RateLimiter::new();
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        for _ in 0..MAX_AUTH_FAILURES {
            assert!(rl.check(&ip));
            rl.record_failure(&ip);
        }
        assert!(!rl.check(&ip));

        rl.record_success(&ip);
        assert!(rl.check(&ip));
    }
}
