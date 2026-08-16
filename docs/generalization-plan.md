# 程序通用化计划（Blender 专属 → 通用文件进度板）

> 目标：剥离 `.blend` 硬编码，成为通用"文件版本管理 + 进度时间线 + Web 部署"工具：
> 任意文件类型可配置预览（3D / 图片 / 视频 / 无），核心逻辑（Lore VCS、回退、部署、开关）零改动，旧项目完全兼容。
>
> 起始版本：0.3.2 · 创建：2026-08-14

## 总体架构

```
文件扩展名 → PreviewProvider（预览器）
├── glb    ：3D 模型（Blender 导出 GLB——现有能力）
├── image  ：图片/设计稿（PSD/AI/KRA/Sai/PNG → 缩略图）
├── video  ：视频工程（PR/达芬奇 → 封面帧）
└── none   ：无预览（纯时间线）
```

- 配置：项目级 `preview.toml`（或 MetaConfig 扩展），默认内置映射，用户可增删
- 数据：`timeline.json` 条目加 `preview_type`（"glb"/"image"/"video"/""）；旧数据空 → 前端按 preview 后缀推断
- 兼容：旧项目零迁移；`.blend` 行为完全不变

## 预览格式默认映射（已确认）

| 类型 | 扩展名 | 说明 |
|---|---|---|
| glb | `.blend`（当前）→ 后续 `.fbx` `.glb` `.gltf` | 3D 模型 |
| image | `.png` `.jpg/.jpeg` `.webp` `.tga` `.exr`（贴图/参考图，3D 工作流核心）；`.psd` `.kra` `.sai` `.clip`（设计源文件，解析失败降级） | 缩略图 |
| video | `.mp4` `.mov` `.webm`（取首帧）；`.prproj` `.aep` `.drp`（工程封面，起步 PR/达芬奇） | 封面帧 |

原则：默认只放高频格式；配置/文档/压缩包类不映射（预览价值低）。

## 阶段状态

### 阶段 1：配置化 + Provider 抽象 —— ✅ 已完成（0.3.2，commit 4cc4fcf，后续继续完善）

- [x] `src/preview.rs`：`PreviewKind` / `PreviewConfig` / `PreviewProvider` trait / `GlbProvider`
- [x] record 与 html_gen 预览判定改用配置（`kind_of`）
- [x] `timeline.rs`：`preview_type` 字段（serde default）+ rebuild_cache 沿用
- [x] 前端 `preview.ts`：`isPreviewable()` 替代 `.blend` 硬编码
- [x] 项目级预览配置：`.keinestamp/preview.toml`（`PreviewConfig::load/save`，缺失用默认，解析失败降级默认）
- [x] Provider 分发：`PreviewConfig::provider_for()` + `PreviewProvider::output_subdir()`，record 不再硬编码 GlbProvider
- [x] `preview_type` 实际落库：新记录生成预览时写入；旧缓存无该字段时按 preview 产物后缀推断
- [x] html_gen 使用项目配置，GLB 产物名改为通用去扩展名逻辑（支持自定义 `.fbx` 等映射）
- [x] 行为与之前完全一致（179 测试通过）

### 阶段 2：图片预览（image provider）—— ✅ 基础完成（待体验完善）

- [x] `ImageProvider`：PNG/JPG/JPEG/WebP 直接复制原图（保留透明通道/画质，缩略图优化后续再做）
- [x] TGA/EXR：通过 ffmpeg 转 JPG（无 ffmpeg 时降级无预览）
- [x] PSD：方案 A——解析 Image Resources 提取内嵌 JPEG 缩略图（`psd_thumbnail.rs`），失败后尝试 ffmpeg 兜底
- [x] 配置默认集：`.png .jpg .jpeg .webp .tga .exr`（+ `.psd .kra .sai .clip` 尽力而为）
- [x] web 模板 `preview-box`：type=image 渲染 `<img>`（懒加载）
- [x] 前端 `preview.ts`：image/video 扩展名可标记预览
- [x] 多预览：一条记录可保存多个图片/视频/GLB 预览，并在 GUI 中逐个开关
- 验收：设计稿/贴图文件可标记预览，部署页显示缩略图

### 阶段 3：视频工程预览（video provider）—— ✅ 基础完成（工程封面待做）

- [x] `.mp4/.mov/.webm` 通过 ffmpeg 取首帧生成 JPG 封面（无 ffmpeg 时降级无预览）
- [ ] `.prproj`（PR XML 内封面图引用）、`.drp`（达芬奇）解析工程内嵌预览图——暂未实现
- [x] 解析失败降级无预览
- [x] web 模板 `preview-box`：type=video 渲染封面图
- [x] `.glb` 文件可直接作为 3D 预览（复制到 previews_glb，不再强制 Blender 转换）
- 验收：视频文件记录后部署页显示封面图

### 阶段 4：体验收尾 —— ⏳ 待执行

- 预览文件类型管理**作为程序预设**维护（不开放用户配置 UI；特殊格式走专用 Provider 特殊处理）
- preview-box 图标/标签完善（Model/Image/Video）
- 前端 `preview.ts` 与后端默认映射的同步维护说明

## 兼容性与风险

| 项 | 处理 |
|---|---|
| 旧项目 timeline.json | 无 `preview_type` → 按 preview 后缀推断，零迁移 |
| 部署页 3D 性能 | 重构保留按需渲染/防抖/瘦身成果（回归测试） |
| PSD 解析 | `psd` crate 支持有限 → 失败降级，不阻塞主流程 |
| 视频解码 | 起步解析工程封面（零解码），直接取帧需 ffmpeg 依赖（评估后定） |

## 范围外

团队协作、云端服务、配置 UI 复杂化、`.fbx/.glb/.gltf` 直读（依赖 Blender 导入导出，阶段 2 后评估）。

## 验证与交付

每阶段：`cargo test --lib` + `npm run check` → 提交推送 → dev 验证 → 阶段清单确认。
服务器端无需任何改动（serve 只托管产物）。

## 二期计划（按建议实施顺序）

### P0 预览能力补全
- [x] Krita 原生 `.kra` 预览（ZIP 内提取 `preview.png` / `mergedimage.png`）
- [x] PSD 合并图像数据解码（覆盖无内嵌缩略图 PSD，支持 8-bit RGB/灰度 Raw/RLE）
- [x] PRPROJ / AEP / DRP 工程封面解析（尽力扫描内嵌 JPEG/PNG 作为封面）

### P1 标签 / 工作流
- [x] 三级标签体系：
  - 一级：流程（建模流程 / 绘画流程 / 视频流程）
  - 二级：标签（建模、UV拆分、贴图、草图、上色、剪辑等）
  - 三级：多标签（同一记录可多选多个二级标签）
- [x] 项目级标签组配置（`.keinestamp/tags.toml`，缺失时使用默认三大流程）
- [x] 按文件类型自动建议标签（打开提交弹窗时根据所选文件类型推荐流程与标签）
- [x] 流程模板（默认三大流程即模板，可通过项目标签组覆盖）

### P2 时间线 / 记录体验
- [x] 时间线筛选与搜索（按消息/标签/分类/文件搜索，按流程筛选）
- [x] 记录详情增强（可展开查看完整 ID、时间、流程、标签、文件数、预览数）
- [x] 记录间 Diff / 对比（右键“对比上一记录”，弹窗展示逐文件 diff）
- [x] 空提交 / 纯里程碑记录（Lore 不支持空提交 → `.keinestamp/milestone-marker` 私有 marker + `--force stage`；GUI 空记录先弹确认）
- [x] 回退 / 恢复 UX 完善（回退状态顶部横幅提示 + 恢复按钮）

### P3 部署 / 服务端
- [x] 增量部署（客户端按 FNV 清单对比服务端 `.manifests/<project>.json`，只上传 changed 文件；服务端合并补丁 + 按清单删除多余文件）
- [x] 部署历史（每次部署保留旧版本快照到 `.history/<project>/<timestamp>`）
- [x] 部署回滚（`POST /<项目名>/rollback` 回滚到最近一次历史快照）
- [x] 站点自定义（serve.toml 支持 `site_name`，首页大标题可配置）
- [x] 多项目概览页（首页自动列出 web_root 下已部署项目并链接）

### P4 客户端体验
- [x] 文件类型图标（变更面板按扩展名显示 Model/Image/Video/文件图标）
- [x] 拖拽排序 / 分组（最近项目支持拖拽排序；时间线按标签分组已有）
- [x] 快捷键（Ctrl+R 刷新状态，Ctrl+Enter 打开记录弹窗）
- [x] 启动与轮询优化（页面不可见时暂停轮询刷新）

### P5 工程健壮性 / 数据
- [x] timeline schema 版本化（新增 `version` 字段，当前版本 2）
- [x] 旧项目自动迁移（rebuild_cache 自动补全多预览/多标签并写入 version=2）
- [x] 预览失败诊断（timeline 新增 `preview_errors`，记录时收集生成失败原因；GUI 卡片/详情/折叠区与部署页展示）
- [x] 测试覆盖补强（空提交 marker、预览错误渲染、增量合并、清单持久化、未跟踪删除过滤、中间记录回退保留早期内容、未勾选删除不提交、无尾斜杠目录过滤、回退后提交保留未追踪内容、时间线过滤文件夹行、回退精确移除目标记录已删除文件、已删除目录路径过滤等新增单测；全量 `cargo test --lib` 200 passed / 0 failed）
- [x] 大文件处理（增量部署清单哈希与 zip 写入均流式处理，避免大预览文件整体载入内存；上传/解压仍受 512MB 上限保护）

## 变更记录

- 2026-08-14：计划创建；阶段 1 完成（0.3.2）；版本号 0.3.1 → 0.3.2
- 2026-08-15：阶段 1 继续完善——增加 `.keinestamp/preview.toml` 配置加载、Provider 分发、`preview_type` 落库/推断、html_gen 使用项目配置
- 2026-08-15：阶段 2/3 基础完成——新增 `ImageProvider`（含 PSD 内嵌缩略图提取）、`VideoProvider`（ffmpeg 取首帧）、web 通用图片/视频预览渲染、前端预览类型标记
- 2026-08-15：多预览支持——timeline 增加 `previews` 列表，一条记录可保存多个 GLB/图片/视频预览并逐个开关；`.glb` 文件可直接选择预览
- 2026-08-15：GUI 图片/视频缩略图显示 + 预览类型标签改为英文 Model/Image/Video
- 2026-08-15：提交标签增加流程分类——建模流程 / 绘画流程 / 视频流程，并持久化 `tag_category`
- 2026-08-15：部署页图片/视频预览改为紧凑网格，点击图片可放大到网页尺寸；3D 预览增加“放大”按钮
- 2026-08-15：预览失败诊断——timeline 新增 `preview_errors`，记录时收集未实现/未生成/生成失败的预览文件原因；重建缓存保留；GUI 卡片显示“⚠ 预览失败”、详情与折叠区展示具体原因，部署页同步展示
- 2026-08-15：空提交 / 纯里程碑记录——Lore 0.8.6 不支持空提交，改用 `.keinestamp/milestone-marker` 私有 marker + `--force stage` 制造可提交改动；时间线/UI 过滤该 marker；GUI 无文件变更时先弹确认再创建空记录
- 2026-08-15：增量部署——客户端生成完整文件清单（FNV-1a 哈希）并从服务端拉取上次清单，只打包 changed 文件上传；服务端 `.manifests/<project>.json` 存储清单，合并补丁并按新清单删除多余文件；回滚/全量上传会清除旧清单避免状态过期
- 2026-08-15：测试覆盖补强 + 大文件处理——新增空提交、预览错误渲染、增量合并、清单持久化等单测；增量部署清单哈希与 zip 写入改为流式，避免大文件整体载入内存
