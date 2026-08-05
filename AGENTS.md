# Keinestamp

Single-binary CLI + Tauri GUI Blender Git Progress Board. Rust Edition 2024.

## Commands

```powershell
cargo test                          # 70 unit + 7 integration (was 74+7, removed url_prefix tests)
cargo test --lib <module>           # Single module (config/detect/init/timeline/...)
cargo test --test cli_tests         # Integration only
cargo run -- init <path>            # CLI entry
cd frontend; npm run check          # svelte-check + tsc (no other lint/format)
```

## Architecture

```
main ──→ init ──→ detect ──→ config
  └──→ git_project ──→ blender_render, timeline, html_gen, git_web
main_helpers ──→ config, timeline
recent, app_config ──→ independent (app-level config dir)
git_helpers ──→ shared by CLI and Tauri
```

Build order (crate): config → detect → init → git_project → git_web → blender_render → timeline → html_gen → git_helpers → app_config → main → main_helpers → recent

## Constraints

- **Git safety**: Only `add/commit/push/pull/reset --soft`. No checkout/branch/merge/rebase/reset --hard.
- **revert guard**: Checks commit count >= 2 before `reset --soft HEAD~1`. `get_commit_count()` in git_project.
- **pull**: Always `git pull --rebase` to avoid merge commits.
- **Entry ID**: Full git commit hash. `get_entry()` supports prefix matching.
- **MetaConfig** (`.keinestamp.toml`): `version`, `init_time`, `project_root`, `web_root`, `access_token`, `project_id`, `site_name`, `site_locked`. Old fields silently ignored via `#[serde(default)]` (`web_remote`/`auto_connect` removed — legacy `web_remote` was a never-effective web git remote, and `auto_connect` moved to AppConfig). `access_token` is per-project, for browser login only. `project_id` is a unique identifier generated on init to prevent cross-project overwrites on server. `site_name` (ASCII `[A-Za-z0-9-_]` only, URL prefix / `X-Project-Name`) locks permanently after first deploy (`site_locked = true`); `config::resolve_site_name()` picks `site_name` → sanitized folder name → `project<id前8>` fallback, shared by deploy + jump button.
- **AppConfig** (`%APPDATA%/Keinestamp/app-config.toml`): `blender_path`, `deploy_token`, `deploy_server_url`, `auto_connect`. Server connection settings are all app-level: `deploy_token` authenticates upload/health, `deploy_server_url` is the server address, `auto_connect` is the global auto-test-on-load switch.
- **auto_connect**: App-level (AppConfig). When true, app tests server connection on every project load. GUI: SettingsDialog checkbox; Tauri commands `get_auto_connect`/`set_auto_connect`; CLI `config auto_connect=` writes app config.
- **serve web_root default**: Auto-generated `serve.toml` uses `<exe_dir>/web-root/` (stable, not CWD); explicit `web_root` in `serve.toml` or `--config` overrides. Deployed sites land in `web_root/<site_name>/`, shared three.js in `web_root/_three/`.
- **Deploy version hint**: Client sends `X-Web-Count` (web-timeline `rev-list --count HEAD`) on upload; server records it in `web_root/.versions.json` and serves it at `GET /<project>/version` (deploy_token Bearer). GUI deploy button turns red (`deployAhead`) when `local > remote ?? 0`, tooltip shows version comparison. Purely a hint — no server-side blocking.
- **Atomic writes**: Temp file + rename (name includes PID + target filename).
- **init idempotent**: Re-init fills gaps, does not overwrite `init_time`.
- **UNC paths**: Frontend `normalizePath()` strips `\\?\` prefix at entry. `open_folder`/`open_web_timeline` also strip before passing to OS commands.
- **Export**: `git ls-tree --name-only` + per-file `git show` (not checkout).
- **Web-timeline/**: Separate git repo, excluded via `.gitignore`.
- **Git user**: `ensure_git_user()` sets fallback name/email before commit if not configured globally.
- **Chinese filenames**: `git -c core.quotepath=false` in `get_changed_files`.
- **CLI output**: Chinese labels (`已提交`, `已撤回`, `已推送到远端`). Log format: `  {short_hash}  {date}  {author}  ({file_count} 文件)`.

## serve.rs

Multi-tenant HTTP with two-token auth (deploy_token + access_token). Dynamic project name routing — project name comes from `X-Project-Name` deploy header, becomes URL prefix automatically. Config via `serve.toml`:

```toml
host = "0.0.0.0"
port = 7799
deploy_token = "hex-token"      # app → server upload/health auth
web_root = "C:/Projects/Alice/web-timeline"
```

- No `serve.toml` → auto-generates one with random `deploy_token` on first run (no `url_prefix` — project names are dynamic)
- All endpoints require valid deploy_token.
- **Dynamic project names**: Deploy sends `X-Project-Name` header. Server creates `web_root/<project_name>/` directory and stores access_token in `HashMap<project_name, token>`.
- **Token separation**: `/upload` and `/health` accept only `deploy_token` (Bearer). Browser endpoints (`/<project>/...`) accept `access_token` (cookie or Bearer).
- **URL prefix routing**: Each project is accessible at `/<project_name>/`. Cookie `keinestamp_token` is scoped to `Path=/<project_name>/` for multi-project isolation.
- Login page at `GET /<project>/login`. On success, sets `keinestamp_token` cookie with `Path=/<project>/` and saves token to `localStorage` for auto-fill.
- Static files: try Bearer → Cookie auth (checks both config access_tokens and stored project tokens via `HashMap`), fallback to login page for browser requests
- Per-user `last_update` timestamps (not global)
- Rate limiter: 10 failures / 60s per IP → 120s block. `retain()` cleanup on each `check()`.
- Bearer deploy_token only for `/upload` and `/health`; Bearer access_token or Cookie for browser endpoints (access_token stored per-project during deploy). No Basic auth support.
- Path traversal: `..` string check + `canonicalize()` prefix check (defense-in-depth) + project name sanitization
- Zip Slip: `canonicalize()` check before extraction
- 100MB upload limit
- Upload response: `{"status":"ok","prefix":"/<project_name>"}`
- HTTP warning printed when no HTTPS

## detect.rs

| Condition | Result |
|---|---|
| Path not a dir | `Fresh` |
| No `.keinestamp.toml`, has `.blend` | `Existing` |

| Has `.keinestamp.toml` | `Initialized(MetaConfig)` |
Non-recursive `*.blend` scan. File path → error.

## serve.toml auto-detect

`serve` command auto-detects `serve.toml` in project root before falling back to `--config` flag.

## has_remote()

Checks for git remote named `origin` specifically (not any remote). Consistent with `push()` which requires `origin`. Tauri command does NOT include `web_remote` in this check.

## Tauri GUI (src-tauri/)

`frontend/` — Svelte 5 + Vite + TypeScript (port 1420). `src-tauri/` — Tauri v2 crate wrapping lib.

```powershell
cargo tauri dev                   # Run dev (port 1420)
cargo tauri build                 # Production
cd frontend; npm install          # Install JS deps (three.js)
cd src-tauri; cargo build         # Backend only
```

### Tauri commands (31)

All in `src-tauri/src/lib.rs`. Wrapped via `map_err(anyhow::Result) -> Result<T, String>`.

| Command | Returns | Notes |
|---|---|---|
| `detect` | `"fresh"\|"existing"\|"initialized"` | |
| `init_project` | `MetaConfig` | + init commit + add_recent |
| `record` | short hash (7) | async, emits `commit-progress` events (5 phases). No auto-deploy. Optional `files` param: selected-files commit (`add_and_commit_files`), otherwise full commit. |
| `push` / `pull` | `()` | push only does git push, no deploy |
| `deploy` | `()` | manual deploy to server (zip upload); on success writes `site_name` (resolved) + `site_locked = true` |
| `test_connection` | `bool` | tests server connectivity via /health with Bearer deploy_token |
| `get_deploy_status` | `{local, remote}` | web-timeline commit count vs server record via `GET /<project>/version` (Bearer deploy_token); remote null = no record |
| `has_remote` | `bool` | checks git `origin` only |
| `revert` | `()` | error `"no commits to revert"` < 2 commits |
| `status` | `{clean, output}` | |
| `read_config` / `update_config` | `MetaConfig` / `()` | key-based string matching (`access_token`, `site_name`); `site_name` key validates ASCII `[A-Za-z0-9-_]` and rejects when locked |
| `get_timeline` | `Timeline` | resolves preview paths to absolute |
| `get_recent` / `add_recent` / `remove_recent` | recent mgmt | |
| `open_folder` / `open_web_timeline` / `open_site` | `()` | platform-specific shell cmds; `open_site(url)` requires http(s):// prefix |
| `delete_entry` | `()` | timeline.json only, not git |
| `get_project_log` | `String[]` | git log lines |
| `get_tracked_files` | `String[]` | walks dir (skips hidden/node_modules/target/web-timeline) |
| `get_file_diff` | `String` | `git show <entry> -- <file>` |
| `get_glb_preview` | `String` path | extracts .blend from git + Blender export |
| `download_blender` | `String` path | emits `blender-download-progress` |
| `list_blender_versions` / `list_installed_blender_versions` | `String[]` | |
| `get_blender_path` / `set_blender_path` | `String` / `()` | AppConfig |
| `get_deploy_token` / `set_deploy_token` | `String` / `()` | AppConfig deploy_token |
| `get_auto_connect` / `set_auto_connect` | `bool` / `()` | AppConfig auto_connect |

### Frontend components

All in `frontend/src/`. Key files:

- `App.svelte` — Main layout, state, 3s status polling
- `SettingsDialog.svelte` — deploy_server_url/deploy_token + Blender download. Connect button with connection test (uses deploy_token), auto-connect toggle (app-level). Save does not close dialog.
- `EntryList.svelte` — Timeline cards with collapsible files/diff/3D sections. Site name + access token sections at bottom (token only after deploy).
- `StatusPanel.svelte` — Changes panel: directory-tree view of working files (`git status --porcelain -uall` so untracked dirs are not collapsed). Per-row checkboxes (default all = full commit; unchecking some → record commits only selected via `files` param, selection persists across 3s auto-refresh). Dir checkbox = select/deselect all files under it (indeterminate when partial), dir row click collapses/expands, file row click toggles. Header shows `selected/total` count + 全选/全不选 buttons.
- `CommitDialog.svelte` — record dialog: message/tag/3D-preview only (file selection lives in StatusPanel); closes after each commit (batch = re-open, selection persists on main panel).
- `statusParse.ts` — shared `parseStatus(output)` used by StatusPanel + CommitDialog.
- `StatusBar.svelte` — Connection indicator (green/red dot) only.
- `ProjectTabs.svelte` — Project tabs bar: scrollable tabs area (`overflow-x: auto` + wheel-scroll action + hover-only thin scrollbar + edge mask fade when scrollable + active tab auto `scrollIntoView`). Active tab shows × (wired to `onRemoveRecent`, reuses confirm logic, `stopPropagation` so it doesn't select). Fixed right actions area (☀/🌙 theme toggle + ⚙ settings, divided by border). Props: `theme`/`onToggleTheme`/`onRemoveRecent`.
- `api.ts` — 31 Tauri invoke wrappers
- `types.ts` — Interfaces

**Dead code (don't use)**: `InitDialog.svelte` (replaced by native `@tauri-apps/plugin-dialog`), `ModelViewer.svelte` (3D preview via browser `openWebTimeline` instead).

### Key design decisions

- **Config split**: Project-level MetaConfig vs app-level AppConfig (blender_path, deploy_token)
- **Token separation**: `deploy_token` (app-level, for upload/health) vs `access_token` (per-project, for browser login). Server routes enforce which token is accepted per endpoint.
- **URL prefix routing**: Each project served at `/<project_name>/` based on `X-Project-Name` deploy header. Cookie scoped to `Path=/<project_name>/`, localStorage key scoped to project name. Multiple projects coexist in one browser.
- **No page_password**: Auth handled by serve.rs Bearer + Cookie. HTTPS via Caddy/nginx.
- **Diff data externalized**: `web-timeline/diffs.json` loaded via fetch()
- **push/pull gated by `has_remote()`**: Buttons only enabled when git `origin` remote exists
- **Notifications**: Toast auto-dismiss after 3s. No manual dismiss.
- **Day/night dual theme**: Day = 人形 (white bg + deep blue + ice blue + red), Night = 白泽 (green-black bg + dark green + green-white + red). `[data-theme="night"]` override block at end of app.scss. Manual ☀/🌙 toggle, localStorage key `keinestamp_theme` (desktop) / `keinestamp_web_theme` (web). Inline pre-read script in `frontend/index.html` head prevents white flash. No system-follow, no animations.
- **Theme toggle location**: ☀/🌙 sits in ProjectTabs fixed right actions area; TitleBar is pure window controls (no theme button).
- **Tag badges**: inline `--tag-c` + CSS `color-mix` — auto-adapts to both themes, no `tagSoft`.
- **Web page theme**: `html_gen.rs` template has `:root`/`[data-theme="night"]` dual palettes; 3D viewer and calendar heat colors stay dark/hardcoded. Login page (`serve.rs login_page_html`) uses the same dual palettes and shares the `keinestamp_web_theme` localStorage key (same origin → theme carries over between login and timeline pages). Timeline page extras: entry card hover lift, empty state with ice-blue clock icon, diff-row hover brightness. Web pages have NO logo (removed; desktop TitleBar keeps it).

## Integration tests

At `tests/cli_tests.rs`. Create real git repos in `$env:TEMP/keinestamp_int_*`. Require `user.email`/`user.name` git config. Binary via `CARGO_BIN_EXE_keinestamp`.

## Test gaps (high-risk)

- `push.rs` — 0 tests (zip creation, auth header)
- `main_helpers.rs` — 0 unit tests
- `serve.rs` — HTTP handlers untested
- `git_helpers.rs` — core functions only integration-tested
- `git_project.rs` — push/pull/remote/has_remote untested

## Known dead code

- `blender_render::render_preview` — never called (only `export_to_glb` is used)
