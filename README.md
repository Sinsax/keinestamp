# Keinestamp

Blender 项目进度看板 — 单二进制 CLI + Tauri GUI，后端基于 Epic Games **Lore**（0.8.6，分块去重 VCS）。

## 功能

- **本地记录**: 一键记录 Blender 项目进度（支持多文件勾选、标签、3D 预览）
- **时间线**: 记录浏览、文件变更与 GLB 3D 预览
- **回退/恢复/顶替**: 回退到任意记录（`branch reset`），可撤销回退，回退后记录即顶替
- **隐藏列表**: 本地隐藏路径（app-config 持久化），`web/` 与 `.keinestamp.toml` 预设不可移除
- **去重存储**: Lore 内容寻址去重，.blend 连存 10 次仓库增量 <1%（实测 99.9%）
- **Web 部署**: 部署到多租户 web 服务器（web 产物进主仓库 `web/`，kind=web 提交），浏览器登录查看时间线
- **局域网备份**: `serve` 拉起 loreserver，备份按钮 = `lore push`

## 前置

- Lore CLI + 服务器（锁版本 v0.8.6）：`C:\Users\para\AppData\Local\lore\bin\`（lore.exe + loreserver.exe）
- loreserver 需在线（无纯本地模式）；`KEINESTAMP_LORE_BIN` / `KEINESTAMP_LORESERVER_BIN` 可覆盖路径
- 命令形态速查表：`docs/lore-commands.md`（实测记录）

## CLI 命令

```
keinestamp init <path>          初始化项目（lore 建仓 + 隐藏列表）
keinestamp record <message>     记录进度
keinestamp status               查看变更（过滤隐藏）
keinestamp log                  查看时间线（过滤 kind=web）
keinestamp rollback <entry>     回退到记录
keinestamp restore              撤销回退
keinestamp export <entry> <dir> 导出记录版本
keinestamp diff <entry> <file>  查看文件差异
keinestamp push|pull            推送到 loreserver / 同步
keinestamp serve [--port 7799]  启动 web 服务器（自动拉起 loreserver）
```

## 开发

```powershell
cargo test              # 单元 + 集成测试（需 loreserver 在线）
cargo tauri dev         # GUI 开发（frontend: Svelte 5）
cd src-tauri; cargo build
cd frontend; npm run check
```

## 架构

```
GUI (Tauri) ── lore_project ── lore_backend（唯一 lore 调用点）── loreserver
                     ├── hidden（隐藏列表）     └── web/（kind=web 提交）
                     └── timeline（web/timeline.json 展示缓存）
serve ── ensure_loreserver + 多租户 HTTP
```

详细约束见 `AGENTS.md`；实施计划见 `docs/superpowers/plans/2026-08-10-keinestamp-lore-v2.md`。

## Linux 服务器部署

`deploy/keinestamp.service`（systemd）用于 web 服务器（serve）：

```bash
sudo cp deploy/keinestamp.service /etc/systemd/system/
# 修改 User/Group 与路径
sudo systemctl daemon-reload
sudo systemctl enable --now keinestamp
```
