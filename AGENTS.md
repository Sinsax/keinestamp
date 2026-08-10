# Keinestamp v2（Lore 后端）

Single-binary CLI + Tauri GUI Blender Git Progress Board，后端为 Epic Games **Lore** 0.8.6（分块去重 VCS）。Rust Edition 2024。

## Commands

```powershell
cargo test                          # 99 unit + 11 integration
cargo test --lib <module>           # Single module (config/detect/init/lore_backend/lore_project/...)
cargo test --test cli_tests         # Integration only
cargo run -- init <path>            # CLI entry
cd src-tauri; cargo build           # Tauri backend only
cd frontend; npm run check          # svelte-check + tsc (no other lint/format)
```

**Lore 依赖**：CLI/服务器二进制位于 `C:\Users\para\AppData\Local\lore\bin\`（lore.exe + loreserver.exe，锁版本 v0.8.6）。所有 lore 操作要求本机 loreserver 在线（无纯本地模式）。`KEINESTAMP_LORE_BIN` / `KEINESTAMP_LORESERVER_BIN` 环境变量可覆盖路径。

## Architecture

```
main ──→ init ──→ detect ──→ config
  └──→ lore_project ──→ lore_backend ──→ loreserver (127.0.0.1:41337/41339)
            ├──→ hidden (app-config 隐藏列表)
            ├──→ timeline (展示缓存 web/timeline.json)
            └──→ html_gen → web/ → commit_web (kind=web)
main_helpers ──→ config, timeline
recent, app_config ──→ independent (app-level config dir)
serve ──→ ensure_loreserver (spawn loreserver 子进程) + 多租户 HTTP
```

Build order (crate): config → detect → init → lore_backend → lore_project → hidden → timeline → html_gen → app_config → main → main_helpers → recent

## Constraints

- **Lore CLI 锁版本 v0.8.6**（Windows x86_64 MSVC），不得用 latest；命令形态以 `docs/lore-commands.md`（Task 1 实测速查表）为准，禁止凭空猜参数
- **lore 调用集中点**：所有 `lore`/`loreserver` 命令必须经 `src/lore_backend.rs`（唯一 spawn 点），其他模块不得直接调
- **revision 定位**：必须用签名（完整或前缀 hash），数字 N 不被接受；数字→签名用 `lore revision find number N`
- **metadata**：用户提交 `kind=user`，web 程序提交 `kind=web`；tag/milestone/message 语义以 revision metadata 为准；timeline.json 仅展示缓存（web 部署用，可重建）
- **隐藏列表**（替代 gitignore/skip-worktree）：app-config 持久化（`hidden_paths`），预设 `web/`、`.keinestamp.toml` 不可移除；前缀匹配（`web/` 匹配 `web/xxx`）
- **回退模型**（替代 v1 stash/refs/三向合并状态机）：
  - 回退到记录 = `lore branch reset <记录sig>`（工作区同步还原；要求无 staged/dirty，dirty 由 lore_project 备份还原处理）
  - 撤销回退 = `branch reset <回退前 HEAD sig>`（marker 存 timeline.json：rollback_point + head_before）
  - 顶替 = 回退后直接 record（新 revision 父 = 回退点，被顶替 revision 从主线消失但数据保留）
  - `lore revision restore` 在 0.8.6 不可用（Invalid head）→ 弃用；`revert` = 撤销某 revision 改动（非撤销回退），冲突用 `resolve mine|theirs`
  - 回退前 dirty 文件：备份复制到临时目录 → `lore file reset` 还原 → branch reset → 恢复备份
- **Git safety 不适用**：v2 无 git；禁止在项目里建 git 仓库（集成测试除外）
- **MetaConfig** (`.keinestamp.toml`)：`version`, `init_time`, `project_root`, `web_root`, `access_token`, `project_id`, `site_name`, `site_locked`（结构与 v1 相同；.keinestamp.toml 在隐藏列表，不参与提交）
- **AppConfig** (`%APPDATA%/Keinestamp/app-config.toml`)：`blender_path`, `deploy_token`, `deploy_server_url`, `auto_connect`, `close_to_tray`, `hidden_paths`, `lore_server_url`
- **web 产物**：`web/` 子目录（主 Lore 仓库内，隐藏），html_gen 输出 + timeline.json 缓存；`commit_web` 自动提交（kind=web）；zip 打包 `web/` 上传（push.rs）；web 版本号 = kind=web revision 计数（`web_revision_count`）
- **Atomic writes**: Temp file + rename (name includes PID + target filename)
- **init idempotent**: Re-init fills gaps, does not overwrite `init_time`
- **UNC paths**: Frontend `normalizePath()` strips `\\?\` prefix at entry. `open_folder`/`open_web_timeline` also strip before passing to OS commands
- **Export**: `lore repository dump` 解析文件树 + 逐文件 `lore file write`（不支持目录级 write）
- **Chinese filenames**: Lore 原生 UTF-8，无需 quotepath 处理
- **CLI output**: 中文标签（`已提交`、`已回退到记录 ...`、`已恢复回退前的状态`、`已推送到远端`）。Log 格式：`  {short_hash}  {date}  {author}  ({file_count} 文件)`
- **serve → loreserver**：serve 启动时探测 41339 health，离线则 spawn loreserver（`--config %APPDATA%/Keinestamp/lore-server-config`，store 持久化于 `lore-server-store`）；serve 退出时 child 不强制 kill
- **备份**：Tauri `backup` 命令 = `lore push`（推 config.toml remote_url 对应服务器）

## serve.rs

多租户 HTTP 服务器（web 部署）+ loreserver 拉起。结构与 v1 相同（两 token 认证、动态项目名路由、X-Project-Name、限速、路径穿越防护、100MB 上传限制），配置 `serve.toml`：

```toml
host = "0.0.0.0"
port = 7799
deploy_token = "hex-token"      # app → server upload/health auth
web_root = "C:/Projects/Alice/web-timeline"
```

## detect.rs

| Condition | Result |
|---|---|
| Path not a dir | `Fresh` |
| No `.keinestamp.toml`, has `.blend` | `Existing` |
| Has `.keinestamp.toml` | `Initialized(MetaConfig)` |

Non-recursive `*.blend` scan. File path → error.

## lore-commands.md

`docs/lore-commands.md` — Lore v0.8.6 全部实测命令形态/输出格式/回退语义/去重率结论（Task 1 产物）。**任何新 lore 命令先补速查表再写代码。**

## Tauri GUI (src-tauri/)

`frontend/` — Svelte 5 + Vite + TypeScript (port 1420). `src-tauri/` — Tauri v2 crate wrapping lib.

```powershell
cargo tauri dev                   # Run dev (port 1420)
cargo tauri build                 # Production
cd frontend; npm install          # Install JS deps (three.js)
cd src-tauri; cargo build         # Backend only
```

### Tauri commands

全部在 `src-tauri/src/lib.rs`，lore 语义（v1 命令名保留，内部换后端）：

| Command | Returns | Notes |
|---|---|---|
| `detect` | `"fresh"\|"existing"\|"initialized"` | `.lore` 目录判断 |
| `init_project` | `MetaConfig` | lore 建仓 + 隐藏列表初始化 + add_recent |
| `record` | short hash (7) | `lore_project::record`；async emits `commit-progress`（5 阶段壳）；files 参数 = selected 提交；成功清回退标记 |
| `push` / `pull` | `()` | `lore push` / `lore sync` |
| `deploy` | `()` | web 部署（zip web/ 上传）；成功写 site_name + site_locked |
| `backup` | `()` | `lore push` 到本机/局域网 loreserver |
| `test_connection` / `get_deploy_status` / `has_remote` | 同 v1 | has_remote 检查 `.lore/config.toml` remote_url |
| `rollback` | `()` | branch reset + dirty 备份 + marker |
| `get_rollback_state` | `{rollback_point, head_matches}` | marker 读取；head_matches = current == rollback_point |
| `restore_rollback` | `()` | 撤销回退（branch reset 回 head_before） |
| `get_rollback_conflicts` / `keep_rollback_file` / `resolve_rollback_conflicts` | `[]` / `()` | v1 兼容签名，Lore 无 git 式冲突（UI 已移除） |
| `status` | `{clean, output}` | `changed_files`（过滤隐藏）；output 中文标签 `未提交的修改：` + `  {A/M/D/??} {path}` |
| `log` / `get_project_log` | `LogEntry[]` / `String[]` | revision_history 过滤 kind=web |
| `read_config` / `update_config` | 同 v1 | site_name 校验 ASCII + 锁定 |
| `get_timeline` | `Timeline` | 读 `web/timeline.json`（含 rollback_marker） |
| `export_entry` | `()` | dump + 逐文件 file write |
| `get_recent` / `add_recent` / `remove_recent` / `reorder_recent` | recent mgmt | |
| `open_folder` / `open_web_timeline` / `open_site` | `()` | 同 v1 |
| `get_ignore_info` | `{tracked: [], ignored, rule_ignored: []}` | ignored = 隐藏列表（hidden.rs load_hidden） |
| `add_to_gitignore` / `add_skip_worktree` / `remove_ignore` | `()` | 均为隐藏列表读写（add=添加路径，remove=移除；命令名保留） |
| `delete_entry` | `()` | timeline.json 删条目（不删 revision）+ 重建页面 + commit_web |
| `set_entry_milestone` | `()` | 写 timeline.json milestone 字段（revision metadata 回写不可行，实测 `metadata set --revision` 报错）+ 重建页面 + commit_web |
| `get_tracked_files` | `String[]` | walks dir（skips hidden/node_modules/target/web） |
| `get_file_diff` | `String` | `lore file diff --source <父> --target <rev>` |
| blender / settings / close_action 命令 | 同 v1 | |

### Frontend components

All in `frontend/src/`（组件在 `lib/`）。Key files:

- `App.svelte` — 主布局、状态、3s 轮询。恢复按钮（`rollback_point && head_matches` 时可用）；备份按钮接线；无冲突对话框（v2 已删 ConflictDialog）
- `lib/SettingsDialog.svelte` — 服务器/Blender 设置 + **隐藏路径管理**（列表 + 添加 + 删除，预设 web/、.keinestamp.toml 不可删）
- `lib/ActionBar.svelte` — 记录 / 恢复 / 备份 / 部署
- `lib/EntryList.svelte` — 时间线卡片（id = lore revision sig）、★ milestone、右键导出/回退、回退点灰显
- `lib/StatusPanel.svelte` — 变更面板（lore status 中文格式解析），勾选提交（files 参数），右键忽略/隐藏
- `lib/CommitDialog.svelte` — 记录弹窗（无回退警告，v2 顶替正常）
- `lib/statusParse.ts` — 解析 `未提交的修改：` + `  {code} {path}` 格式
- `api.ts` — Tauri invoke wrappers；`types.ts` — Interfaces（含 rollback_marker）
- 其余（TitleBar/ProjectTabs/Toast/BusyOverlay/DetailPanel/ConfirmDialog/InputDialog/CloseBehaviorDialog/StatusBar/Sidebar/DirTree）— 同 v1

**Dead code (don't use)**: ConflictDialog.svelte（已删除）；v1 的 git_project/git_helpers/git_web 已删除。

### Key design decisions

- **Config split**: Project-level MetaConfig vs app-level AppConfig（hidden_paths、deploy_token 等）
- **Token separation**: `deploy_token`（app 级，上传/健康）vs `access_token`（项目级，浏览器登录）；serve.rs 按端点区分
- **URL prefix routing**: `/<project_name>/` 动态路由，cookie 按 Path 隔离
- **Diff data externalized**: `web/diffs.json` loaded via fetch()
- **push/pull 门控**: `has_remote` 检查 config.toml remote_url（本机 loreserver 恒在 → 恒 true）
- **Notifications**: Toast auto-dismiss after 3s
- **Day/night dual theme**: 同 v1（人形/白泽，localStorage `keinestamp_theme` / `keinestamp_web_theme`）
- **Web page theme**: html_gen 模板双主题；登录页同主题；无 logo，data-URI favicon；根路径品牌落地页

## Integration tests

At `tests/cli_tests.rs`. Create real lore repos in `$env:TEMP/keinestamp_int_*`. Require loreserver 在线（127.0.0.1:41339）。Binary via `CARGO_BIN_EXE_keinestamp`。

## Test gaps (high-risk)

- `push.rs` — zip creation tested; `deploy_to_server` network path untested
- `main_helpers.rs` — 0 unit tests
- `serve.rs` — HTTP handlers + `ensure_loreserver` spawn 未单测（探测分支手工冒烟过）
- `lore_backend.rs` — revert/export_files 集成测试；loreserver 重启信任链实测通过未落测试
- `lore_project.rs` — undo_rollback 冲突路径（current != rollback_point 报错分支）测试

## Known dead code

- (none — v1 git 后端、ConflictDialog 已删除)
