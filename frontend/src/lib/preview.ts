// 预览类型判定（与后端 PreviewConfig 默认同步；扩展类型时前后端同步修改）
// 阶段 1：仅 .blend → glb（3D 预览）；后续 image/video 加入时在此扩展

const GLB_EXTENSIONS = ['.blend']

/** 文件是否可生成预览（可标记「预览」参与记录时的预览生成） */
export function isPreviewable(path: string): boolean {
  const lower = path.toLowerCase()
  return GLB_EXTENSIONS.some((e) => lower.endsWith(e))
}
