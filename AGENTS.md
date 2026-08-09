# Keinestamp

Single-binary CLI + Tauri GUI Blender Git Progress Board. Rust Edition 2024.

## Commands

```powershell
cargo test                          # 100 unit + 8 integration
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

- **Git safety**: Only `add/commit/push/pull/reset --soft`. No checkout/branch/merge/rebase/reset --hard. Exceptions: `git update-index --skip-worktree/--no-skip-worktree` (local index flag only, used for tracked-file local ignore, zero repo impact, fully reversible); `git stash push/pop/drop` (回退暂存/恢复工作区状态, stash 兜底可逆); `git restore --source=<commit> --staged --worktree` (文件回退, 配合 stash 可逆); `git update-ref refs/keinestamp/*` (本地回退状态标记, 仅项目 .git 内, 可 `-d` 删除); `git show <stash>:<path>` 写回文件 (冲突二选一).
- **rollback mechanism** (replaces old revert): 回退 = 工作区有改动先 `stash push -u` (message 前缀 `keinestamp-rollback`) + `git restore --source=<记录^> --staged --worktree` 将文件还原到所选记录的**父提交** (即记录之前的状态, 可重做该记录内容; 第一条记录回退到 init 提交, root commit 无父则报错); 状态标记在项目 .git 的 `refs/keinestamp/rollback` (回退前 HEAD) 与 `refs/keinestamp/rollback-point` (回退点). **回退状态下再次回退 = 自动先恢复再回退** (rollback 内部先调 restore_rollback, 冲突则中止返回 `CONFLICT|`). 恢复 = 三向合并 (base=回退点, 手工实现: 对比 stash 与回退点差异 + 当前工作区差异, 无冲突则 `git restore --source=<stash>` 合并带回 + `stash drop` + 清 refs; 无 stash 时直接 `restore --source=<回退前 HEAD>`, 期间有改动则报错不覆盖; 恢复保留 skip-worktree 标志与 .gitignore 规则). 冲突 = GUI 逐文件二选一 (`resolve_rollback_conflicts`, 写回用 `git restore --worktree` 过 smudge filter, LFS 文件不落指针文本). 回退状态下**记录 = 顶替**: `git reset --soft <回退点>` 后提交 (新记录父 = 回退点, 被回退记录及之后记录从 git 历史与 timeline 移除), 然后 `clear_rollback` (CLI 与 Tauri record 成功路径均调用). 回退后未记录而 HEAD 前进则无法恢复 (`head_matches=false`, 恢复按钮禁用). 核心在 `git_project.rs`.
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
- **CLI output**: Chinese labels (`已提交`, `已推送到远端`, `已回退到记录 ...`, `已恢复回退前的状态`). Log format: `  {short_hash}  {date}  {author}  ({file_count} 文件)`.

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
- Single server-global `last_update` timestamp on `/health` (updated on any deploy)
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

### Tauri commands (51)

All in `src-tauri/src/lib.rs`. Wrapped via `map_err(anyhow::Result) -> Result<T, String>`.

| Command | Returns | Notes |
|---|---|---|
| `detect` | `"fresh"\|"existing"\|"initialized"` | |
| `init_project` | `MetaConfig` | + init commit + add_recent |
| `record` | short hash (7) | async, emits `commit-progress` events (5 phases). No auto-deploy. Optional `files` param: selected-files commit (`add_and_commit_files`), otherwise full commit. Success clears rollback state (`clear_rollback`). |
| `push` / `pull` | `()` | push only does git push, no deploy |
| `deploy` | `()` | manual deploy to server (zip upload); on success writes `site_name` (resolved) + `site_locked = true` |
| `test_connection` | `bool` | tests server connectivity via /health with Bearer deploy_token |
| `get_deploy_status` | `{local, remote}` | web-timeline commit count vs server record via `GET /<project>/version` (Bearer deploy_token); remote null = no record |
| `has_remote` | `bool` | checks git `origin` only |
| `rollback` | `()` | 回退到指定记录**之前** (父提交): 工作区有改动先 `stash push -u` (前缀 `keinestamp-rollback`), 再 `git restore --source=<记录^> --staged --worktree` 还原文件, 写 `refs/keinestamp/rollback` + `rollback-point`; 第一条记录无父提交则报错 |
| `get_rollback_state` | `{rollback_point, head_matches}` | rollback_point null = 无回退状态; head_matches = HEAD 仍停在回退前 (回退后新记录则为 false, 无法恢复) |
| `restore_rollback` | `()` | 恢复回退前状态 (三向合并); 冲突时返回 `CONFLICT|<files…>` 错误, 前端弹逐文件二选一 |
| `get_rollback_conflicts` | `String[]` | 未解决冲突文件列表 (`git diff --diff-filter=U`) |
| `resolve_rollback_conflicts` | `()` | choices: `[path, "current"\|"latest"]` 逐文件二选一; 全部解决后 drop stash + 清 refs |
| `status` | `{clean, output}` | |
| `log` | `LogEntry[]` | timeline entries (short hash/date/message) |
| `read_config` / `update_config` | `MetaConfig` / `()` | key-based string matching (`access_token`, `site_name`); `site_name` key validates ASCII `[A-Za-z0-9-_]` and rejects when locked |
| `get_timeline` | `Timeline` | resolves preview paths to absolute |
| `export_entry` | `()` | 导出 entry 文件到指定目录 (`git ls-tree` + `git show`) |
| `get_recent` / `add_recent` / `remove_recent` / `reorder_recent` | recent mgmt | |
| `open_folder` / `open_web_timeline` / `open_site` | `()` | platform-specific shell cmds; `open_site(url)` requires http(s):// prefix |
| `get_ignore_info` | `{tracked, ignored}` | tracked via `git ls-files`; ignored = ignore 规则命中 + skip-worktree (S 前缀) |
| `add_to_gitignore` / `add_skip_worktree` / `remove_ignore` | `()` | 忽略管理: .gitignore 规则 / 已跟踪文件本地忽略 (skip-worktree, 零 repo 影响) / 移除忽略 |
| `delete_entry` | `()` | timeline.json only, not git |
| `get_project_log` | `String[]` | git log lines |
| `get_tracked_files` | `String[]` | walks dir (skips hidden/node_modules/target/web-timeline) |
| `get_file_diff` | `String` | `git show <entry> -- <file>` |
| `download_blender` | `String` path | emits `blender-download-progress` |
| `refresh_blender_versions` / `get_cached_blender_versions` / `list_installed_blender_versions` | `String[]` | manual-only check (manual button) / persisted cache (no network) / installed local |
| `get_blender_path` / `set_blender_path` | `String` / `()` | AppConfig |
| `get_deploy_token` / `set_deploy_token` | `String` / `()` | AppConfig deploy_token |
| `get_deploy_server_url` / `set_deploy_server_url` | `String` / `()` | AppConfig deploy_server_url |
| `get_auto_connect` / `set_auto_connect` | `bool` / `()` | AppConfig auto_connect |
| `get_close_action` / `set_close_action` | `bool|null` / `()` | AppConfig close_to_tray（关窗行为：null=未决定，true=后台驻留，false=退出）|
| `resolve_close_action` | `()` | 首次关窗弹窗后的落定：记录选择并执行（true=隐藏窗口，false=退出）|
| `set_entry_milestone` | `()` | toggles `milestone` on timeline.json entry (atomic write); regenerates web page + web commit |

### Frontend components

All in `frontend/src/`. Key files:

- `App.svelte` — Main layout, state, 3s status polling. 恢复 button in `ActionBar` (常驻置灰: 非回退状态/head_matches=false 时禁用), click 直接执行恢复 (无 5s 确认), 冲突时弹 `ConflictDialog` (per-file 二选一).
- `SettingsDialog.svelte` — deploy_server_url/deploy_token + Blender download. Connect button with connection test (uses deploy_token), auto-connect toggle (app-level). Save does not close dialog.
- `ActionBar.svelte` — 主操作区: 记录 (record) / 恢复 (常驻置灰, 回退状态下可用) / 部署 (deploy)（旧导出/撤回按钮已移除, 导出移至 EntryList 右键）.
- `EntryList.svelte` — Timeline cards with collapsible files/diff/3D sections. ★ milestone toggle per card (`set_entry_milestone`); milestone cards get full accent-tinted background + accent border. Right-click context menu per card: 导出 (`export_entry` via InputDialog) / 回退 (`rollback`, 回退状态下禁用; 第一条记录禁用 — 无更早状态). 回退状态下, 比回退点新的记录灰显 (`dimmed` class; 回退点不在时间线时全部灰显). Site name + access token sections at bottom (token only after deploy).
- `StatusPanel.svelte` — Changes panel: directory-tree view of working files (`git status --porcelain -uall` so untracked dirs are not collapsed). Per-row checkboxes (default all = full commit; unchecking some → record commits only selected via `files` param, selection persists across 3s auto-refresh). Dir checkbox = select/deselect all files under it (indeterminate when partial), dir row click collapses/expands, file row click toggles. Header shows `selected/total` count + 全选/全不选 buttons.
- `CommitDialog.svelte` — record dialog: message/tag/3D-preview only (file selection lives in StatusPanel); closes after each commit (batch = re-open, selection persists on main panel). 回退未恢复时显示警告 (`回退未恢复，继续记录将无法再恢复`).
- `statusParse.ts` — shared `parseStatus(output)` used by StatusPanel + CommitDialog.
- `StatusBar.svelte` — Connection indicator (green/red dot) only.
- `ProjectTabs.svelte` — Project tabs bar: scrollable tabs area (`overflow-x: auto` + wheel-scroll action + hover-only thin scrollbar + edge mask fade when scrollable + active tab auto `scrollIntoView`). Active tab shows × (wired to `onRemoveRecent`, reuses confirm logic, `stopPropagation` so it doesn't select). Fixed right actions area (☀/🌙 theme toggle + ⚙ settings, divided by border). Props: `theme`/`onToggleTheme`/`onRemoveRecent`.
- `RollbackDialog.svelte` / `ConflictDialog.svelte` — 恢复确认 (5s 延时按钮) / 冲突逐文件二选一 (`current` 保留当前版 vs `latest` 恢复最新版).
- `api.ts` — 42 Tauri invoke wrappers
- `types.ts` — Interfaces

**Dead code (don't use)**: (none — `InitDialog.svelte`/`ModelViewer.svelte` deleted, replaced by native `@tauri-apps/plugin-dialog` and browser `openWebTimeline`)

### Key design decisions

- **Config split**: Project-level MetaConfig vs app-level AppConfig (blender_path, deploy_token)
- **Token separation**: `deploy_token` (app-level, for upload/health) vs `access_token` (per-project, for browser login). Server routes enforce which token is accepted per endpoint.
- **URL prefix routing**: Each project served at `/<project_name>/` based on `X-Project-Name` deploy header. Cookie scoped to `Path=/<project_name>/`, localStorage key scoped to project name. Multiple projects coexist in one browser.
- **No page_password**: Auth handled by serve.rs Bearer + Cookie. HTTPS via Caddy/nginx.
- **Diff data externalized**: `web-timeline/diffs.json` loaded via fetch()
- **push/pull gated by `has_remote()`**: Tauri `push`/`pull` commands skip when no git `origin` remote (no GUI buttons exist; CLI prints skip message)
- **Notifications**: Toast auto-dismiss after 3s. No manual dismiss.
- **Day/night dual theme**: Day = 人形 (white bg + deep blue + ice blue + red), Night = 白泽 (green-black bg + dark green + green-white + red). `[data-theme="night"]` override block at end of app.scss. Manual ☀/🌙 toggle, localStorage key `keinestamp_theme` (desktop) / `keinestamp_web_theme` (web). Inline pre-read script in `frontend/index.html` head prevents white flash. No system-follow, no animations.
- **Theme toggle location**: ☀/🌙 sits in ProjectTabs fixed right actions area; TitleBar is pure window controls (no theme button).
- **Tag badges**: inline `--tag-c` + CSS `color-mix` — auto-adapts to both themes, no `tagSoft`.
- **Web page theme**: `html_gen.rs` template has `:root`/`[data-theme="night"]` dual palettes; 3D viewer and calendar heat colors stay dark/hardcoded. Login page (`serve.rs login_page_html`) uses the same dual palettes and shares the `keinestamp_web_theme` localStorage key (same origin → theme carries over between login and timeline pages). Timeline page extras: entry card hover lift, empty state with ice-blue clock icon, diff-row hover brightness, ★ milestone badge + red left border (`entry.milestone`). Web pages have NO logo (removed; desktop TitleBar keeps it) but both pages embed the logo as a data-URI favicon (`three_assets::logo_favicon_b64`, compiled via `include_bytes!`). Root path `GET /` serves a static branded landing page (`serve.rs INDEX_HTML_TEMPLATE` + `index_page_html`): editorial asymmetric layout (top brand bar, left-text/right-art hero with moon-phase art + ring decorations + progress tick scale, `$ record <hash> ✓` code block, offset feature cards 01/02/03, eyebrow label, wave band + glow blobs + faint mono code texture background), grayscale palette with a single saturated red accent, dual theme, no project info, no preview-only toggle. The moon-phase art is computed client-side in the page (embedded JS): the visitor's LOCAL day start → phase via synodic month (reference new moon 2000-01-06T18:14Z), 8-phase Chinese names, `#moonLight` path + `#moonName` rendered at load; day-fixed per visitor timezone.

## Integration tests

At `tests/cli_tests.rs`. Create real git repos in `$env:TEMP/keinestamp_int_*`. Require `user.email`/`user.name` git config. Binary via `CARGO_BIN_EXE_keinestamp`.

## Test gaps (high-risk)

- `push.rs` — zip creation tested; `deploy_to_server` network path untested
- `main_helpers.rs` — 0 unit tests
- `serve.rs` — HTTP handlers untested
- `git_helpers.rs` — core functions only integration-tested
- `git_project.rs` — push/pull/remote/has_remote untested

## Known dead code

- (none — `blender_render::render_preview`, `ModelViewer.svelte`, `InitDialog.svelte` removed)
