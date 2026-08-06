# Keinestamp

Blender 项目 Git 进度看板 — 单二进制 CLI + Tauri GUI。

## 功能

- **本地记录**: 一键提交 Blender 项目进度（支持多文件勾选、标签）
- **时间线**: 记录浏览、文件变更与 GLB 3D 预览
- **远程同步**: git push/pull（仅 add/commit/push/pull，安全约束）
- **Web 部署**: 部署到多租户 web 服务器，浏览器登录查看时间线
- **内置服务器**: `serve` 子命令启动多租户 HTTP 服务器

## CLI 命令

```
keinestamp init <path>          初始化项目
keinestamp record <path>        记录进度
keinestamp status|log|push|pull|revert
keinestamp serve [--port 7799]  启动 web 服务器
keinestamp deploy --server <url> 部署到服务器
```

## Linux 服务器部署

见 `deploy/keinestamp.service`（systemd）：

```bash
sudo cp deploy/keinestamp.service /etc/systemd/system/
# 修改 User/Group 与路径
sudo systemctl daemon-reload
sudo systemctl enable --now keinestamp
```

## 开发

```powershell
cargo test              # 单元 + 集成测试
cargo tauri dev         # GUI 开发（frontend: Svelte 5）
```

## 技术栈

Rust 2024 edition · Tauri v2 · Svelte 5 · TypeScript
