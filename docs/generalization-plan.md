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

### 阶段 1：配置化 + Provider 抽象 —— ✅ 已完成（0.3.2，commit 4cc4fcf）

- [x] `src/preview.rs`：`PreviewKind` / `PreviewConfig` / `PreviewProvider` trait / `GlbProvider`
- [x] record 与 html_gen 预览判定改用配置（`kind_of`）
- [x] `timeline.rs`：`preview_type` 字段（serde default）+ rebuild_cache 沿用
- [x] 前端 `preview.ts`：`isPreviewable()` 替代 `.blend` 硬编码
- [x] 行为与之前完全一致（162 测试通过）

### 阶段 2：图片预览（image provider）—— ⏳ 待执行

- `image` crate 生成缩略图（PNG/JPG 直接处理）；PSD 等复杂格式用 `psd` crate，失败降级无预览
- 配置默认集：`.png .jpg .jpeg .webp .tga .exr`（+ `.psd .kra .sai .clip` 尽力而为）
- web 模板 `preview-box`：type=image 渲染 `<img>`（复用懒加载/防抖/按需渲染）
- 验收：设计稿/贴图文件可标记预览，部署页显示缩略图

### 阶段 3：视频工程预览（video provider）—— ⏳ 待执行

- `.mp4/.mov/.webm` 直接取首帧（需 ffmpeg 或 Rust 解码器，评估成本）
- `.prproj`（PR XML 内封面图引用）、`.drp`（达芬奇）解析工程内嵌预览图
- 解析失败降级无预览
- 验收：视频文件记录后部署页显示封面图

### 阶段 4：体验收尾 —— ⏳ 待执行

- 配置 UI（SettingsDialog：预览文件类型管理）
- preview-box 图标/标签完善（3D/图/影）
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

## 变更记录

- 2026-08-14：计划创建；阶段 1 完成（0.3.2）；版本号 0.3.1 → 0.3.2
