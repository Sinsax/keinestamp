import type { TagGroup } from '../api'

/** 旧版内置分类名称：仅用于展示「已经没有对应流程配置」的历史记录 */
const LEGACY_CATEGORY_LABELS: Record<string, string> = {
  blender: 'Blender',
  modeling: '建模流程',
  painting: '绘画流程',
  video: '视频流程',
}

/** 旧分类 id → 新流程 id（默认流程「建模流程」已改名 Blender，旧记录继续归入该流程） */
const CATEGORY_ALIASES: Record<string, string> = {
  modeling: 'blender',
}

/** 把历史分类 id 归一到当前流程 id（无别名时原样返回） */
export function resolveTagCategory(category?: string | null): string {
  if (!category) return ''
  return CATEGORY_ALIASES[category] ?? category
}

/**
 * 流程显示名：优先取项目配置里的流程名称，其次兼容旧版内置分类名。
 * groups 为空时按旧默认名称显示（modeling → 建模流程）。
 */
export function tagCategoryLabel(category?: string | null, groups?: TagGroup[]): string {
  if (!category) return ''
  const resolved = resolveTagCategory(category)
  const group = groups?.find((g) => g.id === resolved)
  if (group) return group.label
  return LEGACY_CATEGORY_LABELS[resolved] || LEGACY_CATEGORY_LABELS[category] || category
}

export const TAG_COLORS: Record<string, string> = {
  '建模': '#3d6fa8',
  'UV拆分': '#3e6b52',
  '绑定': '#7a5fa8',
  '贴图': '#a8722e',
  '修复': '#c03535',
  '草图': '#8a6d3b',
  '线稿': '#4e5f8a',
  '上色': '#a84a6e',
  '细化': '#3b7a6e',
  '完稿': '#5d8a3b',
  '剪辑': '#b0543a',
  '调色': '#6e4a8a',
  '特效': '#2e7a8a',
  '字幕': '#4a6a8a',
  '导出': '#8a6d3b',
}
export const FALLBACK_COLORS = ['#3d6fa8', '#3e6b52', '#7a5fa8', '#a8722e', '#c03535', '#4e8a8a']

export function tagColor(tag: string): string {
  if (TAG_COLORS[tag]) return TAG_COLORS[tag]
  let hash = 0
  for (let i = 0; i < tag.length; i++) hash = ((hash << 5) - hash) + tag.charCodeAt(i)
  return FALLBACK_COLORS[Math.abs(hash) % FALLBACK_COLORS.length]
}
