# 网页 3D 预览性能优化计划（体积 / 传输 / 缓存）

> 目标：让部署站点与本地时间线页面的 3D 预览"打得开、不卡"——首屏模型从 11–18 MB 降到
> 2–3 MB 量级，二次访问零请求，且不影响既有预览功能与测试。
>
> 起始版本：0.5.2 · 创建：2026-11-22 · 触发场景：33_芙姆米_新1皮 站点的模型加载卡顿

## 一、现状实测（2026-11-22）

### 1.1 GLB 体积构成：90% 是几何，不是贴图

| GLB | 总大小 | 贴图 | 几何 | meshes |
|---|---|---|---|---|
| `2c9cbc0` / `9e7281b` | 18.2 MB | 0.2 MB | **18.0 MB** | 25 |
| `cefef64_uv` | 11.1 MB | 2.4 MB | **8.6 MB** | 72 |
| `87c2675` | 7.0 MB | 0.3 MB | **6.6 MB** | 59 |

结论：调贴图质量/尺寸几乎没有收益，必须压几何。

根因：导出脚本的减面是"**每个对象**最多 5 万顶点"（`TARGET_VERTS = 50000`），
对 25–72 个 mesh 的模型意味着上百万顶点总量，既撑大文件也压垮 GPU。

### 1.2 服务端现状（serve.rs 静态托管）

- 只有 `Content-Type` + `X-Content-Type-Options`，**无 `Cache-Control` / `ETag` / `Range`**
- `std::fs::read` 整文件读入内存后整包发送
- 文本资源（`three.cjs` 1.38 MB、`index.html` 184 KB、`timeline.json` 30 KB）同样裸传

→ 每刷新一次页面，全部 GLB 重新下载，浏览器无任何缓存依据。这是"卡"的主因。

### 1.3 实测结果（2026-11-22，同一批 blend 对比）

| 模型 | 当前脚本 | +Draco | +Draco+贴图 2048/85 | 总倍数 |
|---|---|---|---|---|
| `model_uv.blend` | 11.1 MB | 3.48 MB | **1.78 MB** | **6.2×** |
| `model.blend` | 11.2 MB | 3.52 MB | **1.83 MB** | **6.1×** |

- Draco 单独：约 3.2×（几何 8.6 MB → ~1 MB），此时贴图占剩余体积 2/3
- 再加贴图上限：再降约 2×，合计 6×
- 导出耗时 9–14 s（原 10–12 s，几乎无退化）
- 页面实测：1.83 MB 的 Draco GLB 在 http 下成功解码并渲染（`canvas` 出现，无解码报错）
- 全局顶点预算（25 万 / 15 万）实测收益 < 1%（该模型总顶点仅 17 万）→ **不引入**，保留原"每对象 5 万"

## 二、方案分档

### P0（改动小、收益最直接）

| # | 项 | 做法 | 预期收益 | 改动点 |
|---|---|---|---|---|
| 1 | HTTP 强缓存 | `Cache-Control: public, max-age=31536000, immutable`（GLB 名含 revision 前缀，内容不可变）+ `ETag` + `If-None-Match` → 304 | 二次访问 **0 请求** | `serve.rs` |
| 2 | Draco 几何压缩 | Blender `export_draco_mesh_compression_enable=True`，量化 14/10/12 | 几何 **5–8×**：18 MB → 2–3 MB | `blender_render.rs`、`three_assets.rs`、`html_gen.rs` 模板 |
| 3 | 全局减面预算 | "每对象 5 万"改为"整场景 ≤ 20–30 万顶点" | 再降 **3–4×**，同时缓解 GPU 压力 | `blender_render.rs` |

Draco 代价：页面需 `DRACOLoader` + `draco_decoder.wasm`（约 300 KB，一次性强缓存），
解码 50–150 ms。备选 meshopt：压缩 2–4×、decoder 20 KB、解码更快。

### P1（传输层补齐）

| # | 项 | 做法 | 收益 |
|---|---|---|---|
| 4 | 文本预压缩 | 部署时生成 `.br` 旁路，按 `Accept-Encoding` 直接发 | `three.cjs` 1.38 MB → ~350 KB |
| 5 | Range + 流式发送 | 206 支持，边下边解析；服务端不再整包驻留内存 | 大文件首屏更快、内存平稳 |
| 6 | zip 内 GLB 用 Stored | 已压缩二进制再 Deflate 无收益 | 部署 CPU 与包体 |

### P2（体验层，效果最好但改动大）

| # | 项 | 做法 |
|---|---|---|
| 7 | 记录时渲静态缩略图 | Blender 顺手出 1 帧 PNG（~80 KB）；卡片先显示图，**点开才加载 3D** |
| 8 | 点击加载替代入视口加载 | 去掉 `rootMargin: 200px` 预加载，滚动完全不碰 3D |

## 三、验收标准

- [x] 新记录导出的 GLB：几何体积相对现状下降 ≥ 5×，页面能正常解析并渲染（无 Draco 解码报错）
- [x] 旧 GLB（不含 Draco）仍能正常加载（GLTFLoader 挂 DRACOLoader 后仍兼容旧文件）
- [x] 同一 GLB 二次请求返回 `304`（或 `immutable` 命中，无网络传输）
- [x] `cargo test` 全绿；`npm run check` 0 错误
- [x] 端到端实测：真实项目页面滚动后模型成功创建 canvas

## 三之二、实施状态

| 项 | 状态 | 落点 |
|---|---|---|
| Draco 几何压缩 | ✅ 已实现 | `blender_render.rs`（`GLB_EXPORT_VERSION` 4 → 5） |
| 贴图 2048 / 质量 85 | ✅ 已实现 | 同上 |
| DRACOLoader 资产与包装 | ✅ 已实现 | `three_assets.rs`（`three-src/loaders/DRACOLoader.js` + `three-src/draco/*`） |
| 页面挂载 DRACOLoader | ✅ 已实现 | `html_gen.rs` 模板 |
| `Cache-Control` immutable + `ETag`/304 | ✅ 已实现 | `serve.rs`（`is_immutable_asset`） |
| `Range` / 206 | ✅ 已实现 | `serve.rs`（`parse_byte_range`） |
| 文本 gzip（P1-4） | ✅ 已实现（运行时压缩 + 按 ETag 缓存） | `serve.rs`（`is_compressible`/`gzip_cached`） |
| `.cjs` / `.wasm` MIME | ✅ 修正 | `serve.rs`（`mime_type`） |
| 全局顶点预算 | ❌ 实测无收益，不采用 | — |
| 记录时渲静态缩略图（P2-7） | ⬜ 待办 | — |

## 四、已知约束

- **旧记录不会自动变小**：GLB 文件名带 revision hash，只有以后的新记录用新压缩；
  想让历史记录瘦身需另做"重建预览"功能（顺带可修复此前失败记录的预览）。
- **Draco 资产需新增**：`src/three-src/` 目前只有 `three.cjs` + GLTFLoader/OrbitControls/
  BufferGeometryUtils，需补 `DRACOLoader.js` 与 `draco_decoder.wasm`（沿用 `transform_addon_js`
  的 `window.*` 化包装）。
- **`file://` 不支持**：浏览器同源策略拦截本地 GLB，必须走 http（应用内置服务或部署站点）。

## 五、执行顺序

1. Draco 实测（对现有 blend 量真实压缩率与导出耗时，定参数）
2. P0-2 + P0-3：导出脚本压缩 + 全局预算，`GLB_EXPORT_VERSION` 4 → 5
3. P0-1：serve 缓存头与 304
4. 前端/模板接入 DRACOLoader（兼容旧 GLB）
5. P1-4 / P1-5：文本预压缩与流式发送
6. 端到端验证 + 构建
