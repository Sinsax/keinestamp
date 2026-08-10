# Keinestamp v2（Lore 原生重设计）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Keinestamp 的 git 后端整体替换为 Epic Games Lore（0.8.6），按 Lore 能力模型重设计功能与 UI，解决 .blend 工程 30-40GB 空间线性增长问题。

**Architecture:** Lore CLI 子进程封装层（lore_backend.rs）替代 git_helpers/git_project/git_web；回退状态机（stash/refs/手工三向合并）替换为 `revision restore`/`revert`/`branch reset`；web 部署产物并入主 Lore 仓库 `web/` 子目录（metadata `kind=web` 过滤）；serve 拉起 loreserver 子进程实现局域网备份。timeline.json 降级为纯展示缓存，语义数据存 `revision metadata`。

**Tech Stack:** Rust 2024 edition · Tauri v2 · Svelte 5 · TypeScript · Lore CLI 0.8.6（锁版本）

**Spec:** `docs/superpowers/specs/2026-08-10-keinestamp-lore-v2-design.md`

## Global Constraints

- Lore CLI 锁版本 **v0.8.6**（Windows x86_64 MSVC 预编译二进制），不得用 latest
- 所有 Lore 命令调用必须集中在 `src/lore_backend.rs`，其他模块不得直接调 `lore`
- 隐藏列表（本地忽略）持久化在 app 配置（`app-config.toml`），`web/` 为预设隐藏路径
- 所有提交必须带 metadata：用户提交 `kind=user`，web 程序提交 `kind=web`
- timeline.json 只写展示缓存（web 部署用），语义数据（message/tag/milestone）以 `revision metadata` 为准
- 回退流程禁止引入 stash/update-ref/三向合并手工实现（Lore 分支模型替代）
- 无历史迁移：旧 git 工程不导入
- 测试沿用现有模式：单元测试（`cargo test --lib`）+ 集成测试（`tests/cli_tests.rs`，临时目录建仓）
- 提交规范：每任务结束 `git commit -m "feat: ..."`（提交风格跟随 Task 0 的 fork commit）

---

## 文件结构

**新建（Keinestamp-lore）：**
- `src/lore_backend.rs` — Lore CLI 子进程封装（唯一 lore 调用点）
- `src/lore_project.rs` — 业务层（替代 git_project.rs）：record/rollback/export/diff/ignore 过滤
- `src/hidden.rs` — 本地隐藏列表（app-config 持久化）
- `docs/lore-commands.md` — Task 1 产出的命令速查表（后续任务依赖）
- `scripts/verify_lore.ps1` — 验证门脚本

**修改（从 Keinestamp 复制后改造）：**
- `src/main.rs` — CLI 命令改为 lore 语义
- `src/init.rs` — init 创建 Lore 仓库 + 隐藏列表初始化（不再写 .gitattributes）
- `src/config.rs` / `src/app_config.rs` — 隐藏列表、lore 路径、服务器地址配置
- `src/timeline.rs` — 读 revision history + metadata，过滤 kind=web
- `src/serve.rs` — 拉起 loreserver 子进程
- `src/push.rs` — zip 打包 `web/` 子目录（替代 web-timeline 目录）
- `src/html_gen.rs` — 输出目录改为 `web/`（工作区内）
- `src-tauri/src/lib.rs` — git 相关命令替换
- `frontend/src/statusParse.ts` — lore status 输出解析
- `frontend/src/App.svelte` / `ActionBar.svelte` / `EntryList.svelte` / `RollbackDialog.svelte` — 回退交互简化、隐藏管理入口
- `frontend/src/api.ts` / `types.ts` — 命令与类型更新

**删除：**
- `src/git_project.rs`、`src/git_helpers.rs`、`src/git_web.rs`
- `src-tauri` 中 git 状态机相关命令与前端对应 UI

---

## Task 0: 项目骨架初始化

**Files:**
- Create: `F:\para\Code\KeineStamp-lore\`（整目录）

**Interfaces:**
- Produces: 可编译的 Keinestamp v1 副本（后续任务在此改造）

- [ ] **Step 1: 复制项目**

```powershell
robocopy "F:\para\Code\KeineStamp" "F:\para\Code\KeineStamp-lore" /E /XD target .git node_modules .opencode .superpowers web-timeline /XF Cargo.lock
```

- [ ] **Step 2: 验证复制完整性**

```powershell
cd F:\para\Code\KeineStamp-lore; cargo build 2>&1 | Select-Object -Last 3
```
Expected: `Finished`（debug 即可）

- [ ] **Step 3: 初始化 git 仓库并提交**

```powershell
cd F:\para\Code\KeineStamp-lore
git init; git add .; git commit -m "chore: fork Keinestamp v1 as v2 base"
```

---

## Task 1: 验证门（Lore 能力验证 + 命令速查表）

**Files:**
- Create: `scripts/verify_lore.ps1`
- Create: `docs/lore-commands.md`

**Interfaces:**
- Produces: `docs/lore-commands.md` 速查表——**Task 2+ 所有 lore 命令参数与输出解析以本表为准**；止损决策（去重率 <50% 或关键能力缺失 → 停下报告）

- [ ] **Step 1: 下载并安装 Lore CLI v0.8.6**

从 GitHub releases 下载 `lore-v0.8.6-x86_64-pc-windows-msvc.zip`（连同 `loreserver` 与 `liblore`），解压到 `C:\Users\para\AppData\Local\lore\bin`。验证：

```powershell
& "C:\Users\para\AppData\Local\lore\bin\lore.exe" --version
```
Expected: 输出版本串（记录实际格式到速查表）

- [ ] **Step 2: 验证 repository create（本地模式）**

```powershell
# scripts/verify_lore.ps1 — 每节输出 PASS/FAIL，结果追加到 docs/lore-commands.md
$ErrorActionPreference = "Stop"
$lore = "C:\Users\para\AppData\Local\lore\bin\lore.exe"
$dir = Join-Path $env:TEMP "lore_verify_$PID"
New-Item -ItemType Directory -Path $dir | Out-Null

& $lore repository create (Join-Path $dir "proj") 2>&1 | Out-String   # 记录实际命令与输出
& $lore repository list 2>&1 | Out-String                              # 记录列表输出格式
```

- [ ] **Step 3: 验证 status 的 untracked 检测（StatusPanel 依赖）**

在仓库内创建 `a.txt`、`web/b.txt`，运行 `lore status` 与 `lore dirty`，记录输出格式；**确认 untracked 文件是否出现**。

- [ ] **Step 4: 验证忽略规则支持**

`lore repository config get` 尝试 `ignore`、`ignore.paths` 等键；无则记录"无忽略规则，走 GUI 隐藏列表方案"（Global Constraints 已定此路，仅为确认是否有更优原生方案）。

- [ ] **Step 5: 验证 commit + metadata + history**

```powershell
& $lore file stage (Join-Path $dir "proj") a.txt
& $lore commit (Join-Path $dir "proj") -m "test commit" 2>&1 | Out-String   # 记录输出（revision id 格式）
& $lore revision metadata set (Join-Path $dir "proj") <revision> kind user 2>&1 | Out-String
& $lore revision metadata get (Join-Path $dir "proj") <revision> 2>&1 | Out-String
& $lore revision history (Join-Path $dir "proj") 2>&1 | Out-String   # 记录历史输出格式（时间线依赖）
```
Expected: 提交成功、metadata 可写可读、history 输出可解析

- [ ] **Step 6: 验证 restore / revert / branch reset（回退语义）**

三连提交 c1 c2 c3 后：

```powershell
& $lore revision restore (Join-Path $dir "proj") c1 2>&1 | Out-String   # 工作区应还原到 c1
& $lore revision revert (Join-Path $dir "proj") <c2> 2>&1 | Out-String  # 撤销回退语义
& $lore branch reset (Join-Path $dir "proj") <branch> <c1> 2>&1 | Out-String  # 顶替语义
```
Expected: 各命令行为符合预期；**记录 restore 是否影响未提交 dirty 文件**（关键：Blender 打开文件场景）

- [ ] **Step 7: 验证 file write 批量导出**

```powershell
& $lore file write (Join-Path $dir "proj") <revision> (Join-Path $dir "export") 2>&1 | Out-String
```
Expected: 导出目录包含该 revision 的文件树；记录命令形态（是否支持目录级导出）

- [ ] **Step 8: 验证 loreserver 独立启动 + push（局域网备份链路）**

用 release 包中 `loreserver.exe` 本地起服务（所需 flags 参考 `docs/how-to/deploy-local-lore-server`）；客户端 `repository clone` / `push` 到该服务。Expected: push 成功、可 clone 回；记录认证/端口配置方式。

- [ ] **Step 9: 去重率验证（止损线）**

用真实 .blend（或用户提供样本）连存 10 次（每次小改动数据块）：
统计 `.lore` 仓库总大小 vs 10 × 文件大小之和。
Expected: **去重率 ≥50%**（仓库增量 < 全量的 50%），否则停止并报告用户。

- [ ] **Step 10: 验证中文文件名**

中文文件名文件 commit + status + restore，记录输出编码（预期原生 UTF-8，无需 quotepath 处理）。

- [ ] **Step 11: 汇总速查表 + 止损检查**

全部 PASS/FAIL 与真实命令输出整理进 `docs/lore-commands.md`（分节：create/status/commit/metadata/history/restore/revert/branch_reset/file_write/server/push/去重率）。**任一关键能力 FAIL 或去重率不达标 → 停止，向用户报告。** 全部通过 → 提交：

```bash
git add scripts/verify_lore.ps1 docs/lore-commands.md
git commit -m "docs: lore v0.8.6 capability verification report"
```

---

## Task 2: lore_backend.rs 基础封装

**Files:**
- Create: `src/lore_backend.rs`
- Modify: `src/lib.rs`（声明 `pub mod lore_backend;`）

**Interfaces:**
- Consumes: `docs/lore-commands.md`（§create/status/commit/metadata/history 命令形态）
- Produces（后续任务依赖的精确签名）:
  - `pub fn run_lore(project_root: &Path, args: &[&str]) -> Result<String>`
  - `pub fn repo_exists(project_root: &Path) -> bool`
  - `pub fn create_repo(project_root: &Path) -> Result<()>`
  - `pub fn status_lines(project_root: &Path) -> Result<Vec<LoreStatusLine>>`；`pub struct LoreStatusLine { pub path: String, pub state: LoreState }`；`pub enum LoreState { Added, Modified, Deleted, Untracked, Unchanged }`
  - `pub fn commit(project_root: &Path, message: &str, metadata: &[(&str, &str)]) -> Result<String>`（返回 revision id）
  - `pub fn revision_history(project_root: &Path) -> Result<Vec<LoreRevision>>`；`pub struct LoreRevision { pub id: String, pub message: String, pub metadata: HashMap<String, String> }`

- [ ] **Step 1: 写失败测试（repo_exists + create_repo）**

```rust
// src/lore_backend.rs 末尾
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_lore_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_create_repo_and_detect() {
        let root = tmp_root("create");
        assert!(!repo_exists(&root));
        create_repo(&root).unwrap();
        assert!(repo_exists(&root));
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --lib lore_backend::tests::test_create_repo_and_detect`
Expected: FAIL（模块不存在）

- [ ] **Step 3: 实现 run_lore / repo_exists / create_repo**

```rust
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use anyhow::{bail, Context, Result};

pub const LORE_BIN: &str = "lore"; // Task 1 若安装为非 PATH 路径，改为绝对路径

pub fn run_lore(project_root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new(LORE_BIN)
        .args(args)
        .current_dir(project_root)
        .output()
        .context("failed to spawn lore")?;
    if !out.status.success() {
        bail!("lore {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn repo_exists(project_root: &Path) -> bool {
    project_root.join(".lore").is_dir()
}

pub fn create_repo(project_root: &Path) -> Result<()> {
    // 命令形态以 docs/lore-commands.md 实测为准
    run_lore(project_root, &["repository", "create", "."])?;
    Ok(())
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test --lib lore_backend::tests::test_create_repo_and_detect`
Expected: PASS

- [ ] **Step 5: 写失败测试（commit + revision_history + metadata）**

```rust
    #[test]
    fn test_commit_and_history_with_metadata() {
        let root = tmp_root("commit");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "hello").unwrap();
        commit(&root, "first", &[("kind", "user")]).unwrap();
        let hist = revision_history(&root).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].metadata.get("kind").map(String::as_str), Some("user"));
    }
```

- [ ] **Step 6: 运行确认失败**（`commit` 未定义）

- [ ] **Step 7: 实现 commit / revision_history / status_lines**

```rust
pub fn commit(project_root: &Path, message: &str, metadata: &[(&str, &str)]) -> Result<String> {
    // stage 全部（排除项在 lore_project 层处理）：命令形态以速查表 §stage/commit 为准
    run_lore(project_root, &["file", "stage", ".", "."])?;
    let out = run_lore(project_root, &["commit", ".", "-m", message])?;
    let rev = parse_revision_id(&out); // 速查表记录的实际输出解析（如冒号后 hash 行）
    for (k, v) in metadata {
        run_lore(project_root, &["revision", "metadata", "set", ".", &rev, k, v])?;
    }
    Ok(rev)
}

fn parse_revision_id(out: &str) -> String {
    // 以速查表实测输出为准；兜底：取输出最后一行 trim
    out.lines().last().unwrap_or_default().trim().to_string()
}

pub fn revision_history(project_root: &Path) -> Result<Vec<LoreRevision>> {
    let out = run_lore(project_root, &["revision", "history", "."])?;
    parse_history(&out)
}

// parse_history/parse_status 按速查表实测输出格式实现；
// metadata 缺失时逐 revision 调 metadata get 补全（数量少，可接受）
pub fn status_lines(project_root: &Path) -> Result<Vec<LoreStatusLine>> {
    let out = run_lore(project_root, &["status", "."])?;
    parse_status(&out)
}
```

- [ ] **Step 8: 运行确认通过**

Run: `cargo test --lib lore_backend`
Expected: PASS（若速查表显示命令形态不同，按实测修正后再 PASS）

- [ ] **Step 9: 提交**

```bash
git add src/lore_backend.rs src/lib.rs
git commit -m "feat: lore_backend CLI wrapper with repo/status/commit/history"
```

---

## Task 3: 隐藏列表机制 + lore_project 记录流程

**Files:**
- Create: `src/hidden.rs`
- Create: `src/lore_project.rs`
- Modify: `src/lib.rs`（`pub mod hidden; pub mod lore_project;`）
- Modify: `src/app_config.rs`（hidden 列表持久化字段）

**Interfaces:**
- Consumes: Task 2 的 `lore_backend` 全部函数；v1 的 `timeline.rs` 条目写入接口
- Produces:
  - `pub fn load_hidden(project_root: &Path) -> Vec<String>` / `pub fn save_hidden(project_root: &Path, list: &[String]) -> Result<()>` / `pub fn is_hidden(path: &str, list: &[String]) -> bool`（前缀匹配，`web/` 预设）
  - `pub fn record(project_root: &Path, message: &str, selected: Option<&[String]>, hidden: &[String]) -> Result<String>` — 全量或按 selected 提交（自动排除 hidden）；写 timeline 缓存
  - `pub fn changed_files(project_root: &Path, hidden: &[String]) -> Result<Vec<LoreStatusLine>>` — status_lines 过滤 hidden

- [ ] **Step 1: 写失败测试（hidden.rs）**

```rust
// src/hidden.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hidden_prefix_and_web_preset() {
        let list = vec!["web/".to_string(), "cache".to_string()];
        assert!(is_hidden("web/index.html", &list));
        assert!(is_hidden("cache/f.txt", &list));
        assert!(!is_hidden("models/a.blend", &list));
    }
}
```

- [ ] **Step 2: 运行确认失败** → `hidden` 模块不存在

- [ ] **Step 3: 实现 hidden.rs**

```rust
use std::path::Path;
use anyhow::Result;

pub const WEB_DIR: &str = "web/";

pub fn load_hidden(_project_root: &Path) -> Vec<String> {
    // 从 app-config（app_config.rs 新增字段 hidden_paths: Vec<String>）读取；
    // 尚未实现时返回默认 [WEB_DIR]
    vec![WEB_DIR.to_string()]
}

pub fn save_hidden(_project_root: &Path, _list: &[String]) -> Result<()> {
    Ok(()) // app_config 持久化接入在 Step 7
}

pub fn is_hidden(path: &str, list: &[String]) -> bool {
    list.iter().any(|h| path == h || path.starts_with(h.as_str()))
}
```

- [ ] **Step 4: 运行确认通过** → `cargo test --lib hidden`

- [ ] **Step 5: 写失败测试（changed_files 过滤 + record 提交）**

```rust
// src/lore_project.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lore_backend::{self, create_repo, revision_history};
    use std::path::PathBuf;

    fn tmp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("keinestamp_lp_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_changed_files_filters_hidden() {
        let root = tmp_root("cf");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "1").unwrap();
        std::fs::create_dir_all(root.join("web")).unwrap();
        std::fs::write(root.join("web/b.txt"), "2").unwrap();
        let files = changed_files(&root, &vec!["web/".to_string()]).unwrap();
        assert!(files.iter().all(|f| !f.path.starts_with("web/")));
        assert!(files.iter().any(|f| f.path == "a.txt"));
    }

    #[test]
    fn test_record_commits_and_kind_metadata() {
        let root = tmp_root("rec");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "x").unwrap();
        record(&root, "msg", None, &vec!["web/".to_string()]).unwrap();
        let hist = revision_history(&root).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].metadata.get("kind").map(String::as_str), Some("user"));
    }
}
```

- [ ] **Step 6: 运行确认失败** → `changed_files`/`record` 未定义

- [ ] **Step 7: 实现 lore_project.rs（含 app_config 持久化接入）**

```rust
use std::path::Path;
use anyhow::Result;
use crate::lore_backend::{self, LoreStatusLine};

pub fn changed_files(project_root: &Path, hidden: &[String]) -> Result<Vec<LoreStatusLine>> {
    Ok(lore_backend::status_lines(project_root)?
        .into_iter()
        .filter(|f| !crate::hidden::is_hidden(&f.path, hidden))
        .collect())
}

pub fn record(project_root: &Path, message: &str, selected: Option<&[String]>, hidden: &[String]) -> Result<String> {
    // selected=None → 全量（changed_files 结果全部 stage）；Some → 按勾选 stage（同样过滤 hidden）
    // 实现细节依赖速查表 §stage 命令形态；stage 完成后：
    let rev = lore_backend::commit(project_root, message, &[("kind", "user")])?;
    crate::timeline::rebuild_cache(project_root)?; // v1 已有：从历史重建 timeline.json
    Ok(rev)
}
```

（`timeline::rebuild_cache` 在 Task 5 正式改为从 revision history 重建；此处先调用 v1 的写缓存逻辑。）

- [ ] **Step 8: 运行确认通过** → `cargo test --lib lore_project`

- [ ] **Step 9: 提交**

```bash
git add src/hidden.rs src/lore_project.rs src/lib.rs src/app_config.rs
git commit -m "feat: hidden paths list + record flow on lore backend"
```

---

## Task 4: 回退 / 撤销回退 / 顶替（替代 v1 回退状态机）

**Files:**
- Modify: `src/lore_backend.rs`（新增 restore/revert/branch_reset/current_revision）
- Modify: `src/lore_project.rs`（rollback/undo_rollback/supersede/rollback_state）

**Interfaces:**
- Consumes: Task 1 §restore/revert/branch_reset 实测语义
- Produces:
  - `pub fn restore(project_root: &Path, revision: &str) -> Result<()>` — 工作区还原到指定 revision（对应 v1 rollback 的"还原文件"步）
  - `pub fn revert(project_root: &Path, revision: &str) -> Result<()>` — 撤销回退（对应 v1 restore_rollback）
  - `pub fn branch_reset(project_root: &Path, revision: &str) -> Result<()>` — 回退状态下记录=顶替（对应 v1 reset --soft 顶替）
  - `pub fn rollback_state(project_root: &Path) -> Result<RollbackState>`；`pub struct RollbackState { pub active: bool, pub revision: Option<String> }` — 回退状态用 Lore 分支引用模拟（`keinestamp/rollback` 分支指向回退前 revision）
  - `pub fn record_after_rollback(project_root: &Path, message: &str, hidden: &[String]) -> Result<String>` — 顶替语义：branch_reset 后提交

- [ ] **Step 1: 写失败测试（rollback → undo → supersede 全流程）**

```rust
// src/lore_project.rs tests 追加
    #[test]
    fn test_rollback_undo_supersede_flow() {
        let root = tmp_root("rb");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "v1").unwrap();
        record(&root, "r1", None, &vec![]).unwrap();
        std::fs::write(root.join("a.txt"), "v2").unwrap();
        let r2 = record(&root, "r2", None, &vec![]).unwrap();
        std::fs::write(root.join("a.txt"), "v3").unwrap();
        let r3 = record(&root, "r3", None, &vec![]).unwrap();

        // 回退到 r2（工作区应回到 v2 状态）
        rollback(&root, &r2).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v2");
        assert!(rollback_state(&root).unwrap().active);

        // 撤销回退（回到 v3）
        undo_rollback(&root).unwrap();
        assert!(!rollback_state(&root).unwrap().active);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "v3");

        // 再回退到 r2 然后顶替记录（新记录取代 r3）
        rollback(&root, &r2).unwrap();
        let r4 = record_after_rollback(&root, "r4", &vec![]).unwrap();
        assert_ne!(r4, r3);
        let hist = lore_backend::revision_history(&root).unwrap();
        assert_eq!(hist[0].message, "r4"); // 最新 revision 是 r4（r3 被顶替出主线）
    }
```

- [ ] **Step 2: 运行确认失败** → `rollback`/`undo_rollback`/`record_after_rollback` 未定义

- [ ] **Step 3: 在 lore_backend.rs 实现底层命令**

```rust
pub fn restore(project_root: &Path, revision: &str) -> Result<()> {
    run_lore(project_root, &["revision", "restore", ".", revision])?;
    Ok(())
}

pub fn revert(project_root: &Path, revision: &str) -> Result<()> {
    run_lore(project_root, &["revision", "revert", ".", revision])?;
    Ok(())
}

pub fn branch_reset(project_root: &Path, revision: &str) -> Result<()> {
    run_lore(project_root, &["branch", "reset", ".", "main", revision])?;
    Ok(())
}
```

（命令参数以速查表实测为准；若 restore 对 dirty 文件行为不满足"保留未提交改动"，在 lore_project 层先对 dirty 文件做备份（复制到隐藏临时目录），恢复后合并——实测后按 Task 1 §5 结论调整。）

- [ ] **Step 4: 在 lore_project.rs 实现业务逻辑**

```rust
pub struct RollbackState { pub active: bool, pub revision: Option<String> }

pub fn rollback(project_root: &Path, revision: &str) -> Result<()> {
    lore_backend::restore(project_root, revision)?;
    // 状态标记：用分支引用模拟（分支名 keinestamp/rollback 指向当前 HEAD revision）
    // 实现以速查表 branch create 实测为准；或退化为 timeline.json 缓存字段（rollback_marker）
    Ok(())
}

pub fn undo_rollback(project_root: &Path) -> Result<()> {
    // 撤销 = revert 回退操作本身（语义以 Task 1 §5 实测为准）
    Ok(())
}

pub fn record_after_rollback(project_root: &Path, message: &str, hidden: &[String]) -> Result<String> {
    // 顶替：先 branch_reset 到回退点，再正常 record
    Ok(record(project_root, message, None, hidden)?)
}
```

（v1 的 `get_rollback_state`/`head_matches` 概念映射：`rollback_state()` 从分支引用或缓存字段读取；"head 前进则无法恢复"对应 Task 1 实测的 branch reset 保护语义。）

- [ ] **Step 5: 运行确认通过** → `cargo test --lib lore_project::tests::test_rollback_undo_supersede_flow`

- [ ] **Step 6: 提交**

```bash
git add src/lore_backend.rs src/lore_project.rs
git commit -m "feat: rollback/undo/supersede via lore restore/revert/branch reset"
```

---

## Task 5: 导出 + diff + 时间线重建

**Files:**
- Modify: `src/lore_backend.rs`（export_files/file_diff/file_info）
- Modify: `src/lore_project.rs`（export_entry/get_file_diff）
- Modify: `src/timeline.rs`（rebuild_cache 改为从 revision history + metadata 重建，过滤 kind=web）

**Interfaces:**
- Consumes: Task 1 §file write/diff 实测
- Produces:
  - `pub fn export_files(project_root: &Path, revision: &str, dest: &Path) -> Result<()>`（对应 v1 `export_entry`）
  - `lore_backend::file_diff(project_root, revision, path) -> Result<String>` — 底层 diff；`lore_project::get_file_diff(project_root, revision, path) -> Result<String>` — 业务包装（Tauri `get_file_diff` 命令直接调用）
  - `timeline::rebuild_cache(project_root) -> Result<()>` 改造：读 revision_history，过滤 kind=web，写 timeline.json（字段对齐 v1 的 Timeline/LogEntry 结构）

- [ ] **Step 1: 写失败测试（export + diff）**

```rust
    #[test]
    fn test_export_and_diff() {
        let root = tmp_root("ex");
        create_repo(&root).unwrap();
        std::fs::write(root.join("a.txt"), "one").unwrap();
        record(&root, "e1", None, &vec![]).unwrap();
        std::fs::write(root.join("a.txt"), "two").unwrap();
        let r2 = record(&root, "e2", None, &vec![]).unwrap();

        let dest = tmp_root("ex_out");
        export_files(&root, &r2, &dest).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "two");

        let d = get_file_diff(&root, &r2, "a.txt").unwrap();
        assert!(!d.is_empty());
    }
```

- [ ] **Step 2: 运行确认失败**

- [ ] **Step 3: 实现 export_files / file_diff**

```rust
pub fn export_files(project_root: &Path, revision: &str, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    run_lore(project_root, &["file", "write", ".", revision, dest.to_str().unwrap()])?;
    Ok(())
}

pub fn file_diff(project_root: &Path, revision: &str, path: &str) -> Result<String> {
    run_lore(project_root, &["file", "diff", ".", revision, path])
}
```

（命令参数以速查表实测为准；二进制文件 diff 预期为"有差异"判定而非文本 diff，前端按 v1 行为显示。）

- [ ] **Step 4: 运行确认通过**

- [ ] **Step 5: timeline.rs 重建逻辑改造**

```rust
// rebuild_cache 伪代码（按 v1 Timeline 结构对齐字段）：
// let hist = lore_backend::revision_history(root)?;
// let entries = hist.iter()
//     .filter(|r| r.metadata.get("kind") != Some(&"web".to_string()))
//     .map(|r| LogEntry { id: r.id.clone(), message: r.message.clone(), files: changed_files_in(r), milestone: r.metadata.get("milestone") == Some("true") })
//     .collect();
// 原子写 timeline.json（沿用 v1 atomic write 模式）
```

- [ ] **Step 6: 运行 `cargo test --lib timeline` + 全模块回归，确认 v1 单测中依赖 git 的失败项符合预期（git_project 相关测试在 Task 7 删除）**

- [ ] **Step 7: 提交**

```bash
git add src/lore_backend.rs src/lore_project.rs src/timeline.rs
git commit -m "feat: export/diff + timeline cache rebuilt from lore history"
```

---

## Task 6: web 部署并入主仓库（kind=web）

**Files:**
- Modify: `src/html_gen.rs`（输出目录改为 `web/`）
- Modify: `src/lore_backend.rs`（commit_web 封装）
- Modify: `src/push.rs`（zip 打包 web/ 目录）
- Modify: `src/git_web.rs` → **删除**（web-timeline git 仓库逻辑）
- Modify: `src/lore_project.rs`（web_revision_count：kind=web 计数）

**Interfaces:**
- Consumes: Task 3 hidden 机制（web/ 已预设隐藏）
- Produces:
  - `pub fn commit_web(project_root: &Path) -> Result<()>` — stage web/ 目录 + commit（metadata kind=web）
  - `pub fn web_revision_count(project_root: &Path) -> Result<u64>` — kind=web 的 revision 数（部署版本号，替代 rev-list --count）

- [ ] **Step 1: 写失败测试**

```rust
    #[test]
    fn test_web_commit_and_count() {
        let root = tmp_root("web");
        create_repo(&root).unwrap();
        std::fs::create_dir_all(root.join("web")).unwrap();
        std::fs::write(root.join("web/index.html"), "<html></html>").unwrap();
        commit_web(&root).unwrap();
        assert_eq!(web_revision_count(&root).unwrap(), 1);
        // web 提交不应出现在用户时间线
        let hist = lore_backend::revision_history(&root).unwrap();
        assert!(hist.iter().all(|r| r.metadata.get("kind") != Some(&"user".to_string())));
    }
```

- [ ] **Step 2: 运行确认失败**

- [ ] **Step 3: 实现 commit_web / web_revision_count**

```rust
pub fn commit_web(project_root: &Path) -> Result<()> {
    // stage web/ 下全部文件（命令形态以速查表 §file stage 为准，路径参数 = web/）
    lore_backend::commit(project_root, "web update", &[("kind", "web")])
}

pub fn web_revision_count(project_root: &Path) -> Result<u64> {
    Ok(lore_backend::revision_history(project_root)?
        .iter()
        .filter(|r| r.metadata.get("kind").map(String::as_str) == Some("web"))
        .count() as u64)
}
```

- [ ] **Step 4: 运行确认通过**

- [ ] **Step 5: html_gen 输出路径改为 `web/`（沿用 v1 的页面生成函数，仅改输出目录参数）；push.rs 的 zip 源目录从 web-timeline 改为 `web/`；删除 git_web.rs 及其调用**

- [ ] **Step 6: 编译 + 既有 html_gen/push 单测适配**

Run: `cargo test --lib html_gen; cargo test --lib push`
Expected: PASS

- [ ] **Step 7: 提交**

```bash
git add src/html_gen.rs src/push.rs src/lore_backend.rs src/lore_project.rs src/lib.rs
git rm src/git_web.rs
git commit -m "feat: web deploy artifacts live in main lore repo (kind=web)"
```

---

## Task 7: CLI 命令重写（main.rs）

**Files:**
- Modify: `src/main.rs`（init/record/status/log/rollback/export/diff/push 等命令改调 lore_project）
- Modify: `src/init.rs`（init 建 Lore 仓库 + 初始 kind=user 提交；删除 .gitattributes/LFS 逻辑）
- Modify: `src/detect.rs`（检测 `.lore` 目录替代 `.git`）
- Modify: `src/main_helpers.rs`（按需）
- Delete: `src/git_project.rs`、`src/git_helpers.rs`
- Modify: `tests/cli_tests.rs`（集成测试适配 lore 语义）

**Interfaces:**
- Consumes: Task 2-6 全部 lore_project/lore_backend 函数
- Produces: 与 v1 相同的 CLI 表面（`keinestamp init/record/status/log/rollback/export/push`），行为 lore 化

- [ ] **Step 1: 删除 git_project.rs / git_helpers.rs 并修正引用，使 `cargo build` 失败**

```powershell
Remove-Item src\git_project.rs, src\git_helpers.rs
cargo build 2>&1 | Select-Object -First 20
```
Expected: 编译错误（引用缺失模块）——这就是待重写清单

- [ ] **Step 2: 重写 init 流程（init.rs）**

- 调用 `lore_backend::create_repo` + 初始 `record`（kind=user）
- 删除 LFS/.gitattributes 生成逻辑（v1 init.rs 118-172 行的 filter=lfs 规则块）
- 隐藏列表初始化为 `["web/"]`
- 单测：init 后 `.lore` 存在、无 `.gitattributes`、`repo_exists` true

- [ ] **Step 3: 重写 record/status/log/rollback 命令（main.rs）**

按 v1 的 CLI 结构逐命令替换调用：
- `status` → `changed_files`（过滤隐藏）
- `record` → `record`/`record_after_rollback`
- `log` → `revision_history` 过滤 kind=web（输出格式沿用 v1：`{short_id}  {date}  {message}  ({n} 文件)`）
- `rollback <entry>` → `rollback`；`restore`（恢复）→ `undo_rollback`
- `export <entry> <dir>` → `export_files`
- `push` → `lore push`（Task 8 服务器就绪后；本期先保留命令骨架）

- [ ] **Step 4: 集成测试适配（tests/cli_tests.rs）**

沿用 v1 模式（`CARGO_BIN_EXE_keinestamp` + `$env:TEMP` 临时目录），git 相关断言改为 lore 语义断言：
- init → record（2 次）→ log 条数 2 → rollback → 文件内容还原 → 撤销回退
- record 后 timeline.json 存在且条目数正确

Run: `cargo test --test cli_tests`
Expected: PASS

- [ ] **Step 5: 全量回归 + 提交**

```bash
cargo test
git add src/main.rs src/init.rs src/detect.rs src/main_helpers.rs tests/cli_tests.rs src/lib.rs
git rm src/git_project.rs src/git_helpers.rs
git commit -m "feat: CLI commands on lore backend"
```

---

## Task 8: Tauri 命令重写（src-tauri/src/lib.rs）

**Files:**
- Modify: `src-tauri/src/lib.rs`（51 个命令中 git 相关替换）
- Modify: `frontend/src/api.ts` / `types.ts`（命令签名同步）

**Interfaces:**
- Consumes: Task 2-7 全部 lore 函数
- Produces: Tauri 命令表面保持 v1 名称（前端改动最小化），内部实现换 lore；删除状态机相关命令

- [ ] **Step 1: 逐个替换 git 相关命令**

| v1 命令 | v2 实现 |
|---|---|
| `detect` | `repo_exists`（`.lore` 目录） |
| `init_project` | init 流程（Task 7 Step 2 的函数） |
| `record` | `record`/`record_after_rollback`（保留 files 参数与 commit-progress 事件 5 阶段壳） |
| `status` | `changed_files` + 格式对齐 v1 `{clean, output}` |
| `log`/`get_project_log` | `revision_history`（过滤 kind=web） |
| `rollback`/`get_rollback_state`/`restore_rollback` | `rollback`/`rollback_state`/`undo_rollback` |
| `get_rollback_conflicts`/`keep_rollback_file`/`resolve_rollback_conflicts` | 冲突场景改由 Lore merge 原生处理；先移除按钮，冲突 UX 简化（Task 9） |
| `export_entry`/`get_file_diff` | `export_files`/`file_diff` |
| `get_ignore_info`/`add_to_gitignore`/`add_skip_worktree`/`remove_ignore` | `hidden` 列表读写（load_hidden/save_hidden/is_hidden） |
| `push`/`pull`/`has_remote` | `lore push/sync` + 服务器配置检测（Task 8 完成后启用） |
| `get_timeline` | timeline.rs 缓存读取（不变） |
| `delete_entry`/`set_entry_milestone` | milestone 写入 revision metadata（替代 timeline.json 字段） |
| 其余（blender/recent/config/serve 相关） | 不变 |

- [ ] **Step 2: 编译 + 冒烟**

```powershell
cd src-tauri; cargo build
```
Expected: `Finished`

- [ ] **Step 3: 前端 api.ts/types.ts 同步签名（删除已移除命令的 wrapper，新增 hidden 读写）**

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs frontend/src/api.ts frontend/src/types.ts
git commit -m "feat: tauri commands on lore backend"
```

---

## Task 9: 前端 UI 适配（Svelte 5）

**Files:**
- Modify: `frontend/src/statusParse.ts`（lore status 输出解析）
- Modify: `frontend/src/App.svelte`（回退状态逻辑简化、隐藏管理入口）
- Modify: `frontend/src/ActionBar.svelte`（记录/备份按钮；恢复按钮逻辑简化）
- Modify: `frontend/src/EntryList.svelte`（时间线数据源 + 回退右键；删除恢复状态相关灰显逻辑）
- Modify: `frontend/src/RollbackDialog.svelte`（5s 确认保留；冲突二选一保留，数据源改为 lore 冲突信息）
- Modify: `frontend/src/CommitDialog.svelte`（回退未恢复警告保留）
- Delete: 冲突解析相关 UI（`ConflictDialog.svelte` 若 Lore 合并自动处理则删除；否则保留）

**Interfaces:**
- Consumes: Task 8 的 Tauri 命令签名
- Produces: 与 v1 等价的功能 UI，回退交互大幅简化

- [ ] **Step 1: statusParse.ts 重写**

```typescript
// 按 docs/lore-commands.md §status 实测输出格式实现 parseStatus(output): StatusEntry[]
// StatusEntry 形状对齐 v1（path/state/checked 等），state 枚举映射 lore 的 added/modified/deleted/untracked
```

- [ ] **Step 2: 时间线数据流改造（EntryList/App）**

- `get_timeline` 返回的条目 id 为 lore revision id（可沿用现有卡片渲染）
- milestone 切换走 `set_entry_milestone`（内部写 revision metadata）

- [ ] **Step 3: 回退交互简化（App/ActionBar/RollbackDialog）**

- 回退：右键/按钮 → 确认 → `rollback`（无恢复状态机联动）
- 恢复：`undo_rollback` 按钮，仅在 `rollback_state().active` 时可用（替代 head_matches 判断）
- 删除"回退未恢复继续记录将无法再恢复"的复杂状态提示（v2 顶替是正常操作，无此限制）——以 Task 1 §5 实测为准

- [ ] **Step 4: 隐藏文件管理 UI（App 设置区）**

- 新增"隐藏路径"列表查看/编辑（调用 hidden 读写命令）

- [ ] **Step 5: 验证**

```powershell
cd frontend; npm run check
```
Expected: svelte-check + tsc 通过，无错误

- [ ] **Step 6: 提交**

```bash
git add frontend/src
git commit -m "feat: frontend adapted to lore backend"
```

---

## Task 10: serve 集成 loreserver（局域网备份）

**Files:**
- Modify: `src/serve.rs`（启动时 spawn `loreserver` 子进程，配置内部端口 + 简化认证）
- Modify: `src/lore_backend.rs`（push_to_server/sync_from_server）
- Modify: `src/app_config.rs`（lore_server_url 字段）
- Modify: `src-tauri/src/lib.rs`（backup 命令：push + 部署状态）
- Modify: `frontend/src/ActionBar.svelte` / `api.ts`（备份按钮）

**Interfaces:**
- Consumes: Task 1 §loreserver 实测（端口/认证/启动 flags）
- Produces:
  - `serve` 启动时自动拉起 loreserver（同机局域网，端口如 7800；进程退出时关闭）
  - `pub fn push_to_server(project_root: &Path, server_url: &str) -> Result<()>`（`lore push`）
  - Tauri `backup` 命令：git 侧无（已全 Lore），推送主仓库到局域网服务器

- [ ] **Step 1: 写失败测试（push 链路）**

```rust
// lore_backend tests
    #[test]
    fn test_push_to_server() {
        // 前置：本机已起 loreserver（测试脚本 scripts/start_test_server.ps1 或测试内 spawn）
        // 建仓 → 提交 → push_to_server → 断言无错误且服务端可见（repository list）
    }
```

- [ ] **Step 2: 运行确认失败** → `push_to_server` 未定义

- [ ] **Step 3: 实现 push_to_server + serve 集成**

```rust
pub fn push_to_server(project_root: &Path, server_url: &str) -> Result<()> {
    // 命令形态以速查表 §push 实测为准（含服务器地址参数/认证 token 方式）
    run_lore(project_root, &["push", server_url])?;
    Ok(())
}
```

serve.rs：在现有 HTTP 服务器启动旁，spawn `loreserver`（`std::process::Command`，配置档来自 app_config；`serve` 退出时 kill）。局域网简化认证（内部 token 或同机信任），实现以 Task 1 §8 实测为准。

- [ ] **Step 4: 运行确认通过（含 serve 启动 → 客户端 push → clone 回验证）**

- [ ] **Step 5: GUI 备份按钮接线（backup 命令 + 状态展示），npm run check 通过**

- [ ] **Step 6: 提交**

```bash
git add src/serve.rs src/lore_backend.rs src/app_config.rs src-tauri/src/lib.rs frontend/src
git commit -m "feat: serve hosts loreserver, backup via lore push"
```

---

## Task 11: 收尾（清理 + 文档 + 全量回归）

**Files:**
- Modify: `AGENTS.md`（架构/约束/命令表更新为 lore 语义）
- Modify: `README.md`
- Modify: `PLAN.md`（v2 计划落定）
- Delete: v1 遗留死代码（LFS 相关、git 状态机残留）

- [ ] **Step 1: 死代码清扫**

```powershell
rg -n "git_project|git_helpers|git_web|skip-worktree|lfs|stash" src src-tauri frontend/src
```
清理所有残留引用；`cargo build` + `cargo test` 全绿。

- [ ] **Step 2: 文档更新**

AGENTS.md：架构图（lore_backend/lore_project/hidden/timeline/serve）、命令表、Global Constraints 对应条目；README：技术栈与命令示例更新。

- [ ] **Step 3: 全量验证**

```powershell
cargo test
cd src-tauri; cargo build
cd frontend; npm run check
```
Expected: 全部通过

- [ ] **Step 4: 手工冒烟（GUI）**

`cargo tauri dev`：init 新工程 → record 3 次（含隐藏 web/ 目录）→ 时间线 3 条 → 回退/恢复 → web 部署 → 备份按钮。

- [ ] **Step 5: 提交**

```bash
git add AGENTS.md README.md PLAN.md
git commit -m "docs: keinesStamp v2 lore backend documentation"
```

---

## 执行说明

- Task 1 未通过（关键能力 FAIL 或去重率 <50%）→ **停止**，向用户报告验证结果后决定继续/调整方案
- 所有 `docs/lore-commands.md` 速查表未覆盖的命令细节，先补速查表再写代码（禁止凭空猜测命令参数）
- Task 顺序依赖：2→3→4→5 串行；6 可在 3 后并行；7-9 依赖 2-6；10 依赖 1（loreserver 验证）
- 每个 Task 独立可测可提交，可随时中断

