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

**Lore 依赖**：**编译期自动捆绑**（build.rs 按平台下载 v0.8.6 的 `lore`+`loreserver` 到 `src-tauri/resources/lore-bin/`，sha256 校验，缺失/失败 → 构建失败；Tauri resources 打包、CLI 分发为 exe 同目录 `lore-bin/`）。运行时查找：`KEINESTAMP_LORE_BIN` env → 捆绑目录（Tauri resource_dir 注入 / 编译期注入路径 / exe 同目录）→ 平台默认（Windows `C:\Users\para\AppData\Local\lore\bin\lore.exe` / Linux `lore` PATH）。**启动检测**（`check_lore_env`：`--version` 精确 == 0.8.6）：CLI 命令前、serve 启动（拒绝启动）、GUI 启动（弹窗 + ActionBar 操作禁用）。Lore 为 MIT 许可（© Epic Games, Inc.），捆绑附带 LICENSE。

## Architecture

```
main ──→ init ──→ detect ──→ config
  └──→ lore_project ──→ lore_backend（lore --offline，纯本地）
            ├──→ hidden (app-config 隐藏列表)
            ├──→ timeline (展示缓存 web/timeline.json)
            └──→ html_gen → web/ → commit_web (kind=web)
main_helpers ──→ config, timeline
recent, app_config ──→ independent (app-level config dir)
serve ──→ 多租户 HTTP（web 部署；loreserver 拉起已移除，局域网备份后续按需补）
```

Build order (crate): config → detect → init → lore_backend → lore_project → hidden → timeline → html_gen → app_config → main → main_helpers → recent

## Constraints

- **Lore CLI 锁版本 v0.8.6**（Windows x86_64 MSVC），不得用 latest；命令形态以 `docs/lore-commands.md`（Task 1 实测速查表）为准，禁止凭空猜参数
- **纯本地离线**：`create_repo` = `lore --offline repository create <随机32hex>`（无服务器注册；仓库名限 ASCII，目录可任意含中文）；config.toml remote_url 为空 → push 报 "No remote configured"（局域网备份后续按需补）
- **lore 调用集中点**：所有 `lore`/`loreserver` 命令必须经 `src/lore_backend.rs`（唯一 spawn 点），其他模块不得直接调
- **revision 定位**：必须用签名（完整或前缀 hash），数字 N 不被接受；数字→签名用 `lore revision find number N`
- **metadata**：用户提交 `kind=user`，web 程序提交 `kind=web`；tag/milestone/message 语义以 revision metadata 为准；timeline.json 仅展示缓存（web 部署用，可重建）
- **隐藏/忽略机制**（替代 gitignore/skip-worktree）：
  - 用户自定义忽略（文件/格式/目录）→ **项目级 `.loreignore`**（Lore 原生过滤 status/stage；规则随仓库历史，与 gitignore 同语义；文件须无 BOM）
  - 程序私有目录 `.keinestamp/`（含 `.keinestamp.toml`、`web/` 产物）→ **应用层前缀过滤**（loreignore 不能排除 web，否则 commit_web 无法 stage）；`.keinestamp/.keinestamp.toml` 同时写入 loreignore 预设（init 时 ensure_loreignore 补建，不覆盖用户规则）
  - app-config 的 hidden_paths 已退役
- **回退模型**（v1 语义定稿）：
  - **选 X 回退 = 回退到 X 的父（X-1）**：`branch reset X-1`；X 相对 X-1 的改动（`revision diff` 解析 A/M/D）检出为**真实未提交修改**：
    - M/A 文件：`lore file reset --revision X` 检出（状态"回退改动"，可勾选可记录）
    - D 文件：磁盘保留（状态"回退删除"，只读；**记录时算删除**——record 前物理删除提交）
  - 回退前未提交改动暂存（`.keinestamp/rollback-backup/` + backup-meta.json，移出工作区）；**只备份首次回退**
  - 时间线灰显：**仅 X（superseded_point）灰显不可回退**；其余记录（含 X 之后、被顶替出主线的折叠记录）均可回退；回退任意记录会覆盖 marker（连续回退视为新回退）
  - **已检出 vs 已修改**：检出文件内容与检出时一致（marker `change_hashes` 判定）→ 标签「已检出」（不阻碍连续回退）；用户修改过（内容不符）→ 标签「已修改」
  - **连续回退**（已有回退态再次回退）：**只备份首次回退**；判定"真修改"（`rollback_modified_files`：change_files 工作区 hash ≠ 检出 hash（marker `change_hashes`，检出时算 DefaultHasher），或非检出文件有改动）——仅真修改才弹「丢弃并跳转 / 留下」（`rollback` 带 discard 参数；未确认报"回退需要丢弃确认"）；**纯检出自动丢弃不弹窗**（只显示"已检出"不阻碍回退）；丢弃 = `discard_dirty`（M/D 还原 HEAD + untracked 物理删除，不备份）；**head_before 始终沿用首次回退**（恢复始终回到最初回退前，无中间态残留）；CLI 走「先 undo 再回退」自动恢复路径（main.rs cmd_rollback）
  - **首条记录（无父）回退特例**：`branch reset` 到自身——工作区还原为该记录内容；该记录相对"空"的改动检出后与 HEAD 一致，自然无差异；marker rollback_point = superseded_point = 首条自身
  - **撤销回退（恢复）** = `branch reset <head_before>` + 取回暂存；检出文件（change_files）默认丢弃；
    回退前暂存 ∩ 当前改动冲突 → 弹窗逐文件选择「当前版本改动 / 回退前改动」→ resolve 后恢复
  - 顶替 = 回退状态下 record（新 revision 父 = X-1）；被顶替记录保留时间线（superseded 折叠灰显，时间序插入）
  - marker 存 timeline.json：rollback_point/superseded_point/head_before/deleted_files/change_files
  - `lore revision restore` 在 0.8.6 不可用（Invalid head）→ 弃用；`revert` = 撤销某 revision 改动（非撤销回退），冲突用 `resolve mine|theirs`
  - 点开头文件不可 stage（`Invalid path`）→ `.loreignore` 自动记录用 `lore --force stage`（实测可用）
- **Git safety 不适用**：v2 无 git；禁止在项目里建 git 仓库（集成测试除外）
- **MetaConfig** (`.keinestamp/.keinestamp.toml`)：`version`, `init_time`, `project_root`, `web_root`, `access_token`, `project_id`, `site_name`, `site_locked`（结构与 v1 相同；位于 `.keinestamp/` 私有目录，被 loreignore 排除，不参与提交）
- **AppConfig** (`%APPDATA%/Keinestamp/app-config.toml`)：`blender_path`, `deploy_token`, `deploy_server_url`, `auto_connect`, `close_to_tray`, `lore_server_url`
- **web 产物**：`.keinestamp/web/` 子目录（主 Lore 仓库内，应用层隐藏），html_gen 输出 + timeline.json 缓存；`commit_web` 自动提交（kind=web）；zip 打包上传（push.rs）；web 版本号 = kind=web revision 计数（`web_revision_count`）
- **Atomic writes**: Temp file + rename (name includes PID + target filename)
- **init idempotent**: Re-init fills gaps, does not overwrite `init_time`
- **UNC paths**: Frontend `normalizePath()` strips `\\?\` prefix at entry. `open_folder`/`open_web_timeline` also strip before passing to OS commands
- **Export**: `lore repository dump` 解析文件树 + 逐文件 `lore file write`（不支持目录级 write）
- **Chinese filenames**: Lore 原生 UTF-8，无需 quotepath 处理
- **CLI output**: 中文标签（`已提交`、`已回退到记录 ...`、`已恢复回退前的状态`、`已推送到远端`）。Log 格式：`  {short_hash}  {date}  {author}  ({file_count} 文件)`
- **serve → loreserver**：serve.toml `enable_loreserver = true` 时启动探测 41339 health，离线则 spawn `loreserver --config %APPDATA%/Keinestamp/lore-server-config`（首次生成 local.toml：`[immutable_store.local].path` / `[mutable_store.local].path` → store 持久化于 `lore-server-store`，重启数据保留）；在线则跳过；serve 退出时 kill 拉起的实例（外部已在线不杀）。启动后解析子进程输出打印结构化信息（`parse_loreserver_info`：版本 / store 路径 / auth 状态）+ 常驻读取线程转发后续输出（防管道阻塞）
- **备份**（2026-08-11 实测定稿）：GUI 备份 = `push_to_server`：读配置地址（app-config `lore_server_url`，默认 `lore://127.0.0.1:41337`）→ health 探测（`http://host:41339/health_check`）→ **仓库必须在线创建**（`lore repository create lore://host:port/<名>`，remote_url 自动绑定；**离线仓库 push 输出"Pushed"但数据不到服务器**，备份报"该项目为离线创建"）→ `lore push`（首推自动建分支，增量推送）。CLI `push` 同流程；`lore commit <MESSAGE>` 是位置参数（`-m` 不存在）；`lore repository config set` 不存在（remote_url 改 config.toml）。**loreserver 无认证（auth: None）**——仅限可信局域网，设置 UI 已标注
- **离线仓库迁移**（`migrate_offline_to_online`，实测）：离线仓库无法直接备份——`repository create <url>/<名> --id <同ID>` 预注册（空目录，服务器按 ID 绑定名字，**本地 store/hash/历史全保留**）→ 设 remote_url → push 全量；失败清理服务器残留。GUI：备份时检测离线 → 弹窗「迁移并备份」引导；校验：无回退态
- **3D 预览**：preview 路径存 timeline.json（不在 revision metadata）——rebuild_cache 从旧缓存沿用同 id 条目的 preview（记录后旧记录 3D 标签不丢）；`clean_old_glb_previews` **只删未被 timeline 引用的孤儿 GLB**（被引用文件即使超 max 也保留）
- **启动空态 + 远端标签**（GUI）：**启动空态**（无项目打开时：一行提示 + 新建项目按钮，无卡片页）+ **远端标签**（唯一固定功能标签，**可拖拽排序**，位置存 `localStorage.keinestamp_remote_pos`，虚拟条目 `__keinestamp_remote__` 不写入 recent）：连接状态 + 自动刷新（进入页面拉取）+ 仓库列表（`list_server_repos` = `lore repository list` 解析 `name (id)`）+ 已下载标记（recent 目录名 == 仓库名）+ 多选下载（`clone_server_repo` = `lore clone <url>/<name> <父目录>/<name>` + 重名检测 + `init_project` 补 .keinestamp 补洞 + add_recent）+ 删除（`delete_server_repo` = `lore repository delete <url>/<name>`，**弹窗输入 DELETE 确认**（输入自动转大写），删 = 备份数据丢失）；`localStorage.keinestamp_last_project` + AppConfig `auto_restore_project` 开关自动恢复上次项目；**部署连接独立于部署状态**（autoConn 时打开项目即探测服务器，未部署项目可首次部署）

## serve.rs

多租户 HTTP 服务器（web 部署）+ loreserver 拉起。结构与 v1 相同（两 token 认证、动态项目名路由、X-Project-Name、限速、路径穿越防护、100MB 上传限制），配置 `serve.toml`：

```toml
host = "0.0.0.0"
port = 7799
deploy_token = "hex-token"      # app → server upload/health auth
web_root = "C:/Projects/Alice/web-timeline"
enable_loreserver = true        # 可选：拉起 loreserver（备份服务器，持久化 store，跟随 serve 生命周期）
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
