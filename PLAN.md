# Keinestamp 重构计划

## 目标
客户端→服务端改用 Bearer Token 认证，部署改为手动，拆分软件配置和项目配置。

## 两种 Token

| Token | 作用域 | 用途 | 存在哪里 |
|---|---|---|---|
| 部署 Token | 软件全局 | 客户端→服务端认证，用于上传网页 | `app-config.toml` |
| 访问 Token | 每个项目 | 访客→服务端，查看网页时必须输入 | `.keinestamp.toml` |

## 实施顺序

### Phase 1 — 配置层
1. `app_config.rs` — 新增 `deploy_server_url`、`deploy_token`、`auto_connect`
2. `config.rs` — 新增 `access_token`；删除旧字段
3. `types.ts` — 更新接口定义

### Phase 2 — 服务端认证
4-8. `serve.rs` — Bearer Token + access_token 页面守卫 + /ping

### Phase 3 — 客户端推送
9-11. `push.rs` — Token 认证 + 超时 + test_connection

### Phase 4 — Tauri 命令
12-18. `lib.rs` — 新增 deploy/test_connection/get/update_app_config；重构 commit/push

### Phase 5 — CLI 重构
19-22. `main.rs` — 移除部署逻辑，deploy 改读 AppConfig

### Phase 6 — 前端 GUI
23-27. SettingsDialog / App / ActionBar / api.ts

### Phase 7 — 文档
28. `AGENTS.md`
