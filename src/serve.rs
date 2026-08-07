use crate::three_assets;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};
use tiny_http::{Header, Method, Response, Server};

const MAX_UPLOAD_SIZE: usize = 100 * 1024 * 1024;
const MAX_AUTH_FAILURES: u32 = 10;
const AUTH_WINDOW_SECS: u64 = 60;
const AUTH_BLOCK_SECS: u64 = 120;

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

fn request_ip(request: &tiny_http::Request) -> IpAddr {
    request.remote_addr().map(|sa| sa.ip()).unwrap_or(IpAddr::from([127, 0, 0, 1]))
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
}

fn default_host() -> String { "0.0.0.0".to_string() }
fn default_port() -> u16 { 7799 }

pub fn load_config(path: &Path) -> Result<ServeConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read serve config {:?}", path))?;
    let config: ServeConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse serve config {:?}", path))?;

    if config.deploy_token.is_none() {
        anyhow::bail!("serve.toml missing `deploy_token`");
    }
    if config.web_root.as_os_str().is_empty() {
        anyhow::bail!("serve.toml missing `web_root`");
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
            } else if now.duration_since(tracker.window_start) > Duration::from_secs(AUTH_WINDOW_SECS) {
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
    let ip = request_ip(request);
    if !rate_limiter.check(&ip) {
        return AuthResult::RateLimited;
    }

    let auth_header = request.headers().iter().find(|h| h.field.equiv("Authorization"));
    let auth_value = match auth_header {
        Some(h) => h.value.as_str(),
        None => { rate_limiter.record_failure(&ip); return AuthResult::Unauthorized; }
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
    let ip = request_ip(request);
    if !rate_limiter.check(&ip) {
        return AuthResult::RateLimited;
    }

    let auth_header = request.headers().iter().find(|h| h.field.equiv("Authorization"));

    if let Some(h) = auth_header {
        let value = h.value.as_str();
        if value.starts_with("Bearer ") {
            let token = value.trim_start_matches("Bearer ");
            if project_tokens.read().unwrap().get(project_name).map_or(false, |t| t == token) {
                rate_limiter.record_success(&ip);
                return AuthResult::Authenticated;
            }
            rate_limiter.record_failure(&ip);
            return AuthResult::Unauthorized;
        }
    }

    if let Some(cookie_token) = get_cookie(request, "keinestamp_token") {
        if project_tokens.read().unwrap().get(project_name).map_or(false, |t| t == cookie_token) {
            rate_limiter.record_success(&ip);
            return AuthResult::Authenticated;
        }
    }

    rate_limiter.record_failure(&ip);
    AuthResult::Unauthorized
}

// --- Login page ---

fn login_page_html(project_name: &str) -> String {
    format!(r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>Keinestamp 登录</title>
<script>document.documentElement.dataset.theme = localStorage.getItem('keinestamp_web_theme') === 'night' ? 'night' : 'day';</script>
<style>
:root{{
--bg:#f4f6f8;--surface:#ffffff;--border:#e3e7ed;--text:#1e2833;--text-dim:#6b7684;
--accent:#d94747;--accent-hover:#c63b3b;--accent-soft:#fbeded;
--shadow-lg:0 16px 48px rgba(30,40,51,0.18);
}}
[data-theme="night"]{{
--bg:#141d18;--surface:#1b2620;--border:#3b4f45;--text:#f4faf6;--text-dim:#bccac4;
--accent:#ec6262;--accent-hover:#d94f4f;--accent-soft:#4a2b2d;
--shadow-lg:0 16px 48px rgba(0,0,0,0.55);
}}
*{{box-sizing:border-box}}
body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,"PingFang SC","Microsoft YaHei",sans-serif;display:flex;justify-content:center;align-items:center;min-height:100vh;margin:0;background:var(--bg);color:var(--text)}}
.card{{background:var(--surface);border:1px solid var(--border);padding:40px 36px 32px;border-radius:14px;box-shadow:var(--shadow-lg);text-align:center;width:340px}}
h1{{font-size:20px;margin:0 0 4px;font-weight:700;color:var(--text);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
.sub{{font-size:13px;color:var(--text-dim);margin-bottom:24px}}
input{{width:100%;padding:10px 14px;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--text);font-size:14px;outline:none;transition:border-color .15s,box-shadow .15s}}
input:focus{{border-color:var(--accent);box-shadow:0 0 0 3px color-mix(in srgb,var(--accent) 14%,transparent)}}
button{{margin-top:16px;padding:10px 32px;border:none;border-radius:8px;background:var(--accent);color:#fff;font-size:14px;font-weight:600;cursor:pointer;width:100%;transition:background .15s}}
button:hover{{background:var(--accent-hover)}}
.error{{color:var(--accent);font-size:13px;margin-top:12px;display:none}}
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
</html>"##, project = project_name, favicon = crate::three_assets::logo_favicon_b64())
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
    INDEX_HTML_TEMPLATE.replace("{favicon}", crate::three_assets::logo_favicon_b64())
}

const INDEX_HTML_TEMPLATE: &str = r##" + <!DOCTYPE html>
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
  --bg: #f5f6f8;
  --surface: #ffffff;
  --text: #1b222b;
  --text-dim: #69727c;
  --line: #d9dee5;
  --accent: #d94747;
  --deco-a: #9ab3cd;
  --deco-b: #8fa694;
  --deco-c: #c7a9a9;
  --shadow-lg: 0 16px 48px rgba(27, 34, 43, 0.14);
  --shadow-sm: 0 1px 2px rgba(27, 34, 43, 0.06);
}
[data-theme="night"] {
  --bg: #14171a;
  --surface: #1b1f22;
  --text: #e9efeb;
  --text-dim: #9aa5a0;
  --line: #2a3237;
  --accent: #e05a5a;
  --deco-a: #7fa08d;
  --deco-b: #6e8478;
  --deco-c: #8a6e6e;
  --shadow-lg: 0 16px 48px rgba(0, 0, 0, 0.5);
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
.glow-br { width: 460px; height: 460px; right: -160px; bottom: -160px; border-radius: 50%; background: radial-gradient(circle, color-mix(in srgb, var(--deco-b) 12%, transparent), transparent 70%); filter: blur(40px); }
.wave { top: 0; left: 0; right: 0; height: 140px; opacity: 0.07; }
.wave svg { width: 100%; height: 100%; }
.code-tex { font-family: ui-monospace, "Cascadia Mono", Consolas, monospace; font-size: 10px; color: var(--deco-a); opacity: 0.05; letter-spacing: 0.5px; }
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
  font-size: clamp(52px, 7.5vw, 84px);
  font-weight: 800;
  letter-spacing: -2px;
  margin: 0 0 10px;
  line-height: 1;
}
.hero-sub { font-size: 15px; color: var(--text-dim); margin: 0 0 26px; letter-spacing: 0.3px; }
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
.ha-moon { position: relative; display: flex; align-items: center; justify-content: center; }
.ha-moon svg { display: block; }
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
}

/* ---- 特性卡（错落） ---- */
.cards {
  display: flex; gap: 16px; justify-content: center;
  padding: 0 60px 40px;
  position: relative; z-index: 2;
}
.card {
  width: 200px; padding: 24px 18px;
  background: var(--surface); border: 1px solid var(--line); border-radius: 16px;
  box-shadow: var(--shadow-sm);
  position: relative;
  transition: transform 0.15s ease, box-shadow 0.15s ease, border-color 0.15s ease;
}
.card:hover { transform: translateY(-3px); box-shadow: var(--shadow-lg); border-color: var(--accent); }
.card:hover .card-icon { background: color-mix(in srgb, var(--accent) 15%, transparent); transform: scale(1.05); }
.card-icon { transition: background 0.15s ease, transform 0.15s ease; }
.card:hover .card-num { color: var(--accent); opacity: 1; }
.card-num {
  position: absolute; top: 12px; right: 14px;
  font-family: ui-monospace, Consolas, monospace; font-size: 10px;
  color: var(--text-dim); opacity: 0.6; letter-spacing: 1px;
  transition: color 0.15s ease, opacity 0.15s ease;
}
.card-icon {
  width: 40px; height: 40px; margin-bottom: 14px; border-radius: 10px;
  display: flex; align-items: center; justify-content: center;
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 9%, transparent);
}
.card h3 { font-size: 14px; margin: 0 0 6px; font-weight: 600; }
.card p { font-size: 11.5px; color: var(--text-dim); margin: 0; line-height: 1.55; }

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
      <rect width="100" height="100" rx="22" fill="var(--accent)"/>
    Keinestamp
  </div>
  <div class="topbar-right">Blender Git Progress Board</div>
</header>

<main class="hero">
  <div class="hero-text">
    <div class="eyebrow">Blender · Git · Timeline <span class="dot">●</span></div>
    <h1>Keinestamp</h1>
    <p class="hero-sub">Blender Git Progress Board</p>
    <code class="hero-code"><span style="color:var(--text-dim)">$</span> record 3a1f9c2 <span class="ok">✓ 已记录</span><span class="cursor"></span></code>
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
        <circle cx="50" cy="50" r="42" fill="var(--deco-b)" opacity="0.22"/>
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

<section class="cards">
  <div class="card">
    <span class="card-num">01</span>
    <div class="card-icon"><svg viewBox="0 0 16 16" width="18" height="18" fill="currentColor"><path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 016 13.5V13h-.5a.5.5 0 01-.5-.5V12h-.5a.5.5 0 01-.5-.5V11h-.5a.5.5 0 01-.5-.5V10h-.5a.499.499 0 01-.175-.032l-.179.178a.5.5 0 00-.11.168l-2 5a.5.5 0 00.65.65l5-2a.5.5 0 00.168-.11l.178-.178z"/></svg></div>
    <h3>进度记录</h3>
    <p>将 Blender 项目进展逐条记录为版本</p>
  </div>
  <div class="card">
    <span class="card-num">02</span>
    <div class="card-icon"><svg viewBox="0 0 16 16" width="18" height="18" fill="currentColor"><path d="M3.5 0a.5.5 0 01.5.5V1h8V.5a.5.5 0 011 0V1h1a2 2 0 012 2v11a2 2 0 01-2 2H2a2 2 0 01-2-2V3a2 2 0 012-2h1V.5a.5.5 0 01.5-.5zM1 4v10a1 1 0 001 1h12a1 1 0 001-1V4H1z"/><path d="M4 7h3v1H4zM9 7h3v1H9zM4 10h3v1H4zM9 10h3v1H9z"/></svg></div>
    <h3>版本时间线</h3>
    <p>自动生成时间线与日历热力图</p>
  </div>
  <div class="card">
    <span class="card-num">03</span>
    <div class="card-icon"><svg viewBox="0 0 16 16" width="18" height="18" fill="currentColor"><path d="M8.186 1.113a.5.5 0 00-.372 0L1.846 3.5l1.064 3.739L1.336 14.5h13.328L13.09 7.239 14.154 3.5 8.186 1.113zM8 2.18l4.53 1.79-1.22 4.287a4.5 4.5 0 01-1.242-.42l1.09-3.827a.5.5 0 00-.946-.32L9.2 7.18a4.5 4.5 0 01-2.4 0L7.79 5.69a.5.5 0 00-.947.32l1.09 3.827a4.5 4.5 0 01-1.242.42L5.47 5.97 8 2.18z"/></svg></div>
    <h3>3D 预览</h3>
    <p>模型预览直接内嵌网页</p>
  </div>
</section>

<footer>Keinestamp © 2026</footer>

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
      var kx = Math.abs(k) * 42;
      if (kx < 0.6) kx = 0;
      return 'M 50 8 A 42 42 0 0 ' + (waxing ? 1 : 0) + ' 50 92 A ' + kx.toFixed(3) + ' 42 0 0 ' + (waxing ? 0 : 1) + ' 50 8 Z';
    }
    var now = new Date();
    var day = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 12);
    var info = moonInfo(day);
    document.getElementById('moonLight').setAttribute('d', moonPath(info.phase));
    document.getElementById('moonName').textContent = moonName(info.phase, info.illum);
  })();
</script>

</body>
</html>
 + "##;

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

fn handle_login_post(
    mut request: tiny_http::Request,
    project_name: &str,
    project_tokens: &RwLock<HashMap<String, String>>,
) -> Result<()> {
    let ip = request_ip(&request);
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;

    let token = body
        .split('&')
        .find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            if parts.next()? == "token" { Some(parts.next().unwrap_or("")) } else { None }
        })
        .unwrap_or("");
    let decoded = percent_decode(token);

    let is_valid = project_tokens.read().unwrap().get(project_name).map_or(false, |t| t == &decoded);

    if is_valid {
        log_info("LOGIN", &format!("{} project={} → success", ip, project_name));
        let set_cookie = format!("keinestamp_token={}; Path=/{}/; HttpOnly; SameSite=Lax", decoded, project_name);
        let redirect = Response::from_string("")
            .with_status_code(302)
            .with_header(Header::from_bytes(&b"Location"[..], format!("/{}/", project_name).as_bytes().to_vec()).unwrap())
            .with_header(Header::from_bytes(&b"Set-Cookie"[..], set_cookie.as_bytes()).unwrap());
        request.respond(redirect)?;
    } else {
        log_warn("LOGIN", &format!("{} project={} → failed", ip, project_name));
        let redirect = Response::from_string("")
            .with_status_code(302)
            .with_header(Header::from_bytes(&b"Location"[..], format!("/{}/login?error=1", project_name).as_bytes().to_vec()).unwrap());
        request.respond(redirect)?;
    }
    Ok(())
}

/// Generate a reasonably random hex token (not cryptographically secure, sufficient for LAN use)
pub fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let a = now as u64;
    let b = (pid as u64).wrapping_mul(0x9e3779b97f4a7c15);
    format!("{:016x}{:016x}", a ^ b, a.wrapping_mul(6364136223846793005) ^ b.rotate_left(17))
}

/// Auto-generate `serve.toml` with random tokens when none exists
pub fn auto_generate_config(path: &Path, port: u16, host: &str, web_root: &Path) -> Result<ServeConfig> {
    let dt = generate_token();
    let canonical_root = std::fs::canonicalize(web_root)
        .unwrap_or_else(|_| web_root.to_path_buf());
    let config_toml = format!(
        r#"host = "{}"
port = {}
deploy_token = "{}"
web_root = "{}"
"#,
        host, port, dt,
        canonical_root.to_string_lossy().replace('\\', "\\\\")
    );
    std::fs::write(&path, &config_toml)
        .with_context(|| format!("failed to write auto-generated serve config to {:?}", path))?;
    let config = load_config(path)?;
    let path_str = path.display().to_string();
    let root_str = canonical_root.display().to_string();
    log_info("SETUP", &format!("First run — auto-generated serve.toml at {:?}", path_str));
    log_info("SETUP", &format!("Deploy Token (for app): {}", dt));
    log_info("SETUP", &format!("Web URL: http://{}:{}/<项目名>", host, port));
    log_info("SETUP", &format!("Deploy directory: {} (per project)", root_str));
    log_info("SETUP", "Access tokens are set per-project via GUI and stored on the server during deploy.");
    Ok(config)
}

// --- Server start ---

pub fn start(config: ServeConfig) -> Result<()> {
    let addr = format!("{}:{}", config.host, config.port);
    let server = Server::http(&addr)
        .map_err(|e| anyhow::anyhow!("failed to start server on {}: {}", addr, e))?;

    let web_root = config.web_root.clone();
    let deploy_token = config.deploy_token.clone();

    let three_dir = web_root.join("_three");
    if let Err(e) = three_assets::ensure_threejs(&three_dir) {
        log_error("STARTUP", &format!("failed to generate shared three.js: {}", e));
    } else {
        log_info("STARTUP", "shared three.js assets ready");
    }

    log_info("STARTUP", &format!("Keinestamp serve listening on http://{}", addr));
    log_warn("STARTUP", "HTTP mode — data is sent in plaintext.");
    log_warn("STARTUP", "For production, put behind Caddy/nginx with HTTPS.");
    log_info("STARTUP", &format!("  {} (deploy token)", web_root.display()));

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let last_update = Arc::new(AtomicU64::new(now));
    let project_tokens: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(HashMap::new()));
    let project_ids: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(HashMap::new()));
    let web_versions: Arc<RwLock<HashMap<String, u64>>> = Arc::new(RwLock::new(load_versions(&web_root)));
    let rate_limiter = Arc::new(RateLimiter::new());

    for request in server.incoming_requests() {
        let rl = rate_limiter.clone();
        let dt = deploy_token.clone();
        let pt = project_tokens.clone();
        let pi = project_ids.clone();
        let wv = web_versions.clone();
        let wr = web_root.clone();
        let lu = last_update.clone();
        let method = request.method().clone();
        let url = request.url().to_string();
        let req_ip = request_ip(&request);

        // /health
        if url == "/health" && method == Method::Get {
            let auth = check_deploy_auth(&request, dt.as_deref(), &rl);
            match auth {
                AuthResult::Authenticated => {
                    if let Err(e) = handle_health(request, &lu) {
                        log_error("HEALTH", &format!("{} {}", req_ip, e));
                    }
                }
                AuthResult::RateLimited => {
                    log_warn("AUTH", &format!("{} rate limited on /health", req_ip));
                    let _ = request.respond(Response::from_string("429 Too Many Requests").with_status_code(429));
                }
                AuthResult::Unauthorized => {
                    log_warn("AUTH", &format!("{} unauthorized on /health", req_ip));
                    let _ = request.respond(unauthorized_response());
                }
            }
            continue;
        }

        // /upload
        if url == "/upload" && method == Method::Post {
            let auth = check_deploy_auth(&request, dt.as_deref(), &rl);
            match auth {
                AuthResult::Authenticated => {
                    if let Err(e) = handle_upload(request, &wr, &lu, &pt, &pi, &wv) {
                        log_error("DEPLOY", &format!("{} {}", req_ip, e));
                    }
                }
                AuthResult::RateLimited => {
                    log_warn("AUTH", &format!("{} rate limited on /upload", req_ip));
                    let _ = request.respond(Response::from_string("429 Too Many Requests").with_status_code(429));
                }
                AuthResult::Unauthorized => {
                    log_warn("AUTH", &format!("{} unauthorized on /upload", req_ip));
                    let _ = request.respond(unauthorized_response());
                }
            }
            continue;
        }

        // Root homepage
        if (url == "/" || url.is_empty()) && method == Method::Get {
            if let Err(e) = handle_index(request) {
                log_error("INDEX", &format!("{} {}", req_ip, e));
            }
            continue;
        }

        // Browser routes
        let trimmed = url.trim_start_matches('/');
        let slash_pos = trimmed.find('/');
        let (project_name, rest) = match slash_pos {
            Some(pos) => (&trimmed[..pos], &trimmed[pos..]),
            None => (trimmed, "/"),
        };

        if project_name.is_empty() || !project_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            let _ = request.respond(Response::from_string("404 Not Found").with_status_code(404));
            continue;
        }

        let route_path = if rest.is_empty() { "/" } else { rest };

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
                    let _ = request.respond(Response::from_string("429 Too Many Requests").with_status_code(429));
                }
                AuthResult::Unauthorized => {
                    log_warn("AUTH", &format!("{} unauthorized on /version", req_ip));
                    let _ = request.respond(unauthorized_response());
                }
            }
            continue;
        }

        if route_path == "/login" && method == Method::Get {
            if let Err(e) = serve_login_page(request, project_name) {
                log_error("LOGIN", &format!("{} serve_login_page: {}", req_ip, e));
            }
        } else if route_path == "/login" && method == Method::Post {
            if let Err(e) = handle_login_post(request, project_name, &pt) {
                log_error("LOGIN", &format!("{} handle_login_post: {}", req_ip, e));
            }
        } else {
            let auth = check_browser_static_auth(&request, &rl, project_name, &pt);
            match auth {
                AuthResult::Authenticated => {
                    if let Err(e) = handle_static(request, &wr, project_name) {
                        log_error("ACCESS", &format!("{} handle_static: {}", req_ip, e));
                    }
                }
                AuthResult::RateLimited => {
                    log_warn("AUTH", &format!("{} rate limited on project '{}'", req_ip, project_name));
                    let _ = request.respond(Response::from_string("429 Too Many Requests").with_status_code(429));
                }
                AuthResult::Unauthorized => {
                    let is_browser = request.headers().iter()
                        .find(|h| h.field.equiv("Accept"))
                        .map(|h| h.value.as_str().contains("text/html"))
                        .unwrap_or(false);
                    if is_browser && method == Method::Get {
                        log_info("AUTH", &format!("{} unauthorized → login page (project='{}')", req_ip, project_name));
                        if let Err(e) = serve_login_page(request, project_name) {
                            log_error("LOGIN", &format!("{} serve_login_page: {}", req_ip, e));
                        }
                    } else {
                        log_warn("AUTH", &format!("{} unauthorized on project '{}'", req_ip, project_name));
                        let _ = request.respond(unauthorized_response());
                    }
                }
            }
        }
    }

    Ok(())
}

// --- Request handlers ---

fn handle_health(
    request: tiny_http::Request,
    last_update: &AtomicU64,
) -> Result<()> {
    let ip = request_ip(&request);
    let ts = last_update.load(Ordering::Relaxed);
    let body = format!(r#"{{"status":"ok","lastUpdate":{}}}"#, ts);
    let hdr = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
    let response = Response::from_string(body).with_header(hdr);
    log_info("HEALTH", &format!("{} → 200", ip));
    request.respond(response)?;
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
    let ip = request_ip(&request);
    let mut body = Vec::new();
    request
        .as_reader()
        .take(MAX_UPLOAD_SIZE as u64)
        .read_to_end(&mut body)
        .context("failed to read upload body")?;
    let size = body.len();

    if size >= MAX_UPLOAD_SIZE {
        log_warn("DEPLOY", &format!("{} payload too large ({} bytes)", ip, size));
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

    let project_name_raw = request.headers().iter()
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

    // Conflict check: verify project_id matches
    let project_id = request.headers().iter()
        .find(|h| h.field.equiv("X-Project-Id"))
        .map(|h| h.value.as_str())
        .unwrap_or("");
    {
        let ids = project_ids.read().unwrap();
        if let Some(stored_id) = ids.get(&project_name) {
            if stored_id != project_id {
                log_warn("DEPLOY", &format!("{} project='{}' conflict: project_id mismatch", ip, project_name));
                let response = Response::from_string("409 Conflict: project_id mismatch").with_status_code(409);
                request.respond(response)?;
                return Ok(());
            }
        }
    }
    if !project_id.is_empty() {
        project_ids.write().unwrap().insert(project_name.clone(), project_id.to_string());
    }

    if let Some(access_token) = request.headers().iter()
        .find(|h| h.field.equiv("X-Access-Token"))
        .map(|h| h.value.as_str())
        .filter(|v| !v.is_empty())
    {
        project_tokens.write().unwrap().insert(project_name.clone(), access_token.to_string());
        log_info("DEPLOY", &format!("{} project={} stored access_token", ip, project_name));
    }

    log_info("DEPLOY", &format!("{} project={} size={} bytes", ip, project_name, size));

    // Deploy into web_root/<project_name>/ for per-project isolation
    let deploy_dir = web_root.join(&project_name);
    std::fs::create_dir_all(&deploy_dir).context("failed to create deploy dir")?;

    let tmp_parent = deploy_dir
        .parent()
        .unwrap_or(Path::new("."));
    let tmp_dir = tmp_parent.join(format!(".keinestamp-tmp.{}", std::process::id()));
    let bak_dir = tmp_parent.join(format!(".keinestamp-bak.{}", std::process::id()));

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

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let entry_path = file.mangled_name();
        let path_str = entry_path.to_string_lossy();
        if path_str.contains("..") {
            continue;
        }
        let dest = tmp_dir.join(&entry_path);

        let canonical_tmp = tmp_dir.canonicalize().unwrap_or_else(|_| tmp_dir.to_path_buf());
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
            if let Err(e) = std::io::copy(&mut file, &mut out) {
                let _ = std::fs::remove_dir_all(&tmp_dir);
                let msg = format!("extract error at entry {}: {}", i, e);
                log_error("DEPLOY", &format!("{} extract error: {}", ip, msg));
                let response = Response::from_string(msg).with_status_code(500);
                request.respond(response)?;
                return Ok(());
            }
        }
    }

    let web_exists = deploy_dir.exists();
    if web_exists {
        let _ = std::fs::remove_dir_all(&bak_dir);
        if let Err(e) = std::fs::rename(&deploy_dir, &bak_dir) {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            let msg = format!("failed to swap directories: {}", e);
            log_error("DEPLOY", &format!("{} {}", ip, msg));
            let response = Response::from_string(msg).with_status_code(500);
            request.respond(response)?;
            return Ok(());
        }
    }

    if let Err(e) = std::fs::rename(&tmp_dir, &deploy_dir) {
        if web_exists {
            let _ = std::fs::rename(&bak_dir, &deploy_dir);
        }
        let _ = std::fs::remove_dir_all(&tmp_dir);
        let msg = format!("failed to finalize update: {}", e);
        log_error("DEPLOY", &format!("{} {}", ip, msg));
        let response = Response::from_string(msg).with_status_code(500);
        request.respond(response)?;
        return Ok(());
    }

    let _ = std::fs::remove_dir_all(&bak_dir);

    last_update.store(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        Ordering::Relaxed,
    );

    log_info("DEPLOY", &format!("{} project={} → 200 /{}", ip, project_name, project_name));

    let client_count = request.headers().iter()
        .find(|h| h.field.equiv("X-Web-Count"))
        .and_then(|h| h.value.as_str().parse::<u64>().ok());
    if let Some(count) = client_count {
        web_versions.write().unwrap().insert(project_name.clone(), count);
        let snapshot = web_versions.read().unwrap().clone();
        save_versions(web_root, &snapshot);
    }

    let body = format!(r#"{{"status":"ok","prefix":"/{}"}}"#, project_name);
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
    let ip = request_ip(&request);
    let method = request.method().to_string();
    let raw_path = request.url().to_string();
    let prefix_path = format!("/{}", project_name);
    let without_prefix = raw_path.strip_prefix(&prefix_path).unwrap_or(&raw_path);
    let clean_path = if without_prefix == "/" || without_prefix.is_empty() {
        "index.html".to_string()
    } else {
        without_prefix.trim_start_matches('/').to_string()
    };

    let serve_dir = web_root.join(project_name);
    let shared_three = web_root.join("_three");
    let access_msg = format!("{} {} {} → ", ip, method, raw_path);

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
                let body = format!("500 Internal Server Error: {}", e);
                let response = Response::from_string(body).with_status_code(500);
                request.respond(response)?;
            }
        }
    } else {
        let fallback = serve_dir.join("index.html");
        if fallback.exists() {
            match std::fs::read(&fallback) {
                Ok(content) => {
                    let mime = mime_type(&fallback);
                    let hdr =
                        Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
                    let response = Response::from_data(content).with_header(hdr);
                    log_info("ACCESS", &format!("{}200 → index.html ({})", access_msg, mime));
                    request.respond(response)?;
                }
                Err(_) => {
                    log_info("ACCESS", &format!("{}404 (fallback read error)", access_msg));
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
    let canonical_web = web_root.canonicalize().unwrap_or_else(|_| web_root.to_path_buf());
    for port in 47799u16..47810 {
        let addr = format!("127.0.0.1:{}", port);
        let Ok(server) = Server::http(&addr) else { continue };
        let _ = port_tx.send(port);
        for request in server.incoming_requests() {
            let url = request.url().to_string();
            let clean_path = if url == "/" || url.is_empty() {
                "index.html".to_string()
            } else {
                url.trim_start_matches('/').to_string()
            };
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
                        let hdr = Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
                        let response = Response::from_data(content).with_header(hdr);
                        let _ = request.respond(response);
                    }
                    Err(_) => {
                        let _ = request.respond(Response::from_string("500").with_status_code(500));
                    }
                }
            } else {
                let _ = request.respond(Response::from_string("404").with_status_code(404));
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
    fn index_page_has_brand_and_features() {
        let html = index_page_html();
        assert!(html.contains("Keinestamp"));
        assert!(html.contains("Blender Git Progress Board"));
        assert!(html.contains("data-theme"));
        assert!(html.contains("keinestamp_web_theme"));
        assert!(html.contains("data:image/png;base64,"), "favicon embedded");
        assert!(html.contains("进度记录"));
        assert!(html.contains("版本时间线"));
        assert!(html.contains("3D 预览"));
        assert!(html.contains("card-icon"), "feature cards present");
        assert!(html.contains("eyebrow"), "eyebrow label present");
        assert!(html.contains("hero-code"), "code block present");
        assert!(html.contains("ha-moon"), "moon art present");
        assert!(html.contains("ha-scale"), "progress scale present");
        assert!(html.contains("ha-ring"), "ring decoration present");
        assert!(html.contains("ha-tag"), "moon phase label present");
        assert!(html.contains("moonLight"), "client-side moon path element");
        assert!(html.contains("elongation"), "client-side moon algorithm");
        assert!(html.contains("moonName"), "client-side moon name element");
        assert!(!html.contains("theme-toggle"), "no preview-only toggle in production");
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
        let dir = std::env::temp_dir().join(format!("keinestamp_versions_missing_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load_versions(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_load_config() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("keinestamp_test_serve_{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let config_path = dir.join("serve.toml");
        std::fs::write(&config_path, r#"
host = "127.0.0.1"
port = 8888
deploy_token = "abc123"
web_root = "/tmp/web"
"#)?;
        let config = load_config(&config_path)?;
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8888);
        assert_eq!(config.deploy_token.as_deref(), Some("abc123"));
        assert_eq!(config.web_root, Path::new("/tmp/web"));
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn test_load_config_no_token_fails() {
        let dir = std::env::temp_dir().join(format!("keinestamp_test_serve_notok_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("serve.toml");
        std::fs::write(&config_path, r#"
web_root = "/tmp/web"
"#).unwrap();
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
