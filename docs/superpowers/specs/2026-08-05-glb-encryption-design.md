# GLB 加密设计（防君子档位，纯客户端）

日期：2026-08-05
状态：已获用户批准（"纯加密吧计划一下"，不含水印）

## 目标

部署到服务器的 3D 预览 GLB 文件以密文形式存在，持有访问令牌的访客**能看不能拿**：页面内 3D 渲染正常，但直接下载得到的永远是密文。定位为"防君子"——挡普通用户顺手下载、爬虫批量抓取、URL 分享；不挡会逆向 JS 的技术人员（浏览器端解密密钥必然可达）。

**明确不做**：水印、服务端解密、按访客动态授权、密钥轮换命令。

## 现状链路（核实结论）

1. 客户端：`.blend` 提交 → Blender 导出 `web_root/previews_glb/<hash>_<name>.glb`（blender_render.rs `export_to_glb`，CLI main.rs:200 与 Tauri lib.rs:112 两条路径）
2. `html_gen.rs` 把 `{entry → [{file, url: ./previews_glb/xxx.glb}]}` 内嵌进 index.html 的 `glbData`（html_gen.rs:48-62）
3. 浏览器 `GLTFLoader.load(url)` 直接拉取明文 GLB（html_gen.rs:546-568）
4. 部署：`push.rs::create_zip` 全目录递归打包 → POST 服务器（zip 自动包含新文件，无需改动）
5. 服务器静态服务：令牌门禁（cookie/Bearer）；未知扩展名有 `application/octet-stream` 兜底（serve.rs:785），`.enc` 无需服务器改动
6. 额外风险源：`web-timeline/` 独立 git 仓库若配了 `web_remote` 推送，明文 GLB 会出本地 —— 加密后该仓库也只有密文

## 威胁模型与档位（已确认：档位 1）

| 档位 | 密钥位置 | 防住 | 防不住 |
|---|---|---|---|
| 1（选定） | 每文件随机密钥，加密后内嵌 index.html | 右键另存为、爬虫批量抓、发 URL 给别人 | 会逆向 JS 的人 |
| 2 | f(访问令牌) 派生，改令牌即吊销 | 同上 | 同上 |
| 3 | 服务器限速颁发 | 批量抓取 | 写脚本的人 |

## 加密设计

### 加密点：`export_to_glb` 成功之后（唯一咽喉）

- 算法：AES-256-GCM（自带认证，防篡改）
- 每文件独立随机 32B key + 12B IV
- 产物：`previews_glb/<name>.glb.enc`；**明文 .glb 立即删除**
- 密钥清单：`previews_glb/keys.json`，结构：
  ```json
  {"<glb_name>": {"key": "<base64 32B>", "iv": "<base64 12B>"}}
  ```
- 随部署走（zip 全目录递归），即"钥匙串"
- CLI / Tauri record / get_glb_preview 重导三条路径自动覆盖

### 页面侧（html_gen.rs）

- `glbData` URL 指向 `.glb.enc`，并内嵌 `key`/`iv`
- 查看器替换 `loader.load(url)` 为：`fetch → crypto.subtle.importKey/decrypt (AES-GCM) → GLTFLoader.parse(arrayBuffer, '', onLoad, onError)`（GLB 自包含，path 传空串；three.js GLTFLoader.parse 支持 ArrayBuffer）
- 解密失败 → 走现有 `showError('3D 模型解析失败')`
- **向后兼容**：keys.json 中没有对应条目的文件（旧明文 .glb）自动回退明文 URL —— 旧部署不坏，新导出自动加密

### 清理与轮换

- `clean_old_glb_previews`：同时清理 `.glb` 与 `.glb.enc`，之后 `prune_manifest`（删除 keys.json 中 .enc 已不存在的条目）
- `export_to_glb` 的"已存在则跳过"判断改查 `.glb.enc`
- `get_glb_preview` 快速路径（lib.rs:588-592）改查 `.glb.enc`，返回 `.enc` 路径（前端 UI 无消费方，接口签名不变）
- 密钥轮换 = 删除 previews_glb 后由内置 Blender 自动按需重导（无专门命令）

### 新增依赖（Cargo.toml）

- `aes-gcm`（RustCrypto，纯 Rust）、`rand`、`base64`

## 测试

- Rust 单测：加密→解密 roundtrip；错误密钥解密失败；密文篡改失败（GCM 认证）；manifest 读写与修剪
- Rust 集成：html_gen 输出断言（.enc URL + key 内嵌；无 manifest 时回退明文 URL）；clean_old_glb_previews 对伪造 .enc 的清理
- 手动验证：记录→部署→浏览器查看 3D 正常→直接下载 .glb.enc 确认是密文
- serve.rs / push.rs 零改动（已核实兜底 mime 与递归 zip）
