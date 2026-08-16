// 预览类型判定（与后端 PreviewConfig 默认同步；扩展类型时前后端同步修改）
// 阶段 2/3：glb（3D）、image（图片/设计稿）、video（视频/工程封面）

export type PreviewType = 'glb' | 'image' | 'video' | ''

const GLB_EXTENSIONS = ['.blend', '.glb']
const IMAGE_EXTENSIONS = [
  '.png', '.jpg', '.jpeg', '.webp', '.tga', '.exr',
  '.psd', '.kra', '.sai', '.clip',
]
const VIDEO_EXTENSIONS = [
  '.mp4', '.mov', '.webm', '.prproj', '.aep', '.drp',
]

/** 根据源文件扩展名推断预览类型 */
export function previewTypeOf(path: string): PreviewType {
  const lower = path.toLowerCase()
  if (GLB_EXTENSIONS.some((e) => lower.endsWith(e))) return 'glb'
  if (IMAGE_EXTENSIONS.some((e) => lower.endsWith(e))) return 'image'
  if (VIDEO_EXTENSIONS.some((e) => lower.endsWith(e))) return 'video'
  return ''
}

/** 文件是否可生成预览（可标记「预览」参与记录时的预览生成） */
export function isPreviewable(path: string): boolean {
  return previewTypeOf(path) !== ''
}
