export const TAG_COLORS: Record<string, string> = {
  '建模': '#3d6fa8',
  'UV拆分': '#3e6b52',
  '绑定': '#7a5fa8',
  '贴图': '#a8722e',
  '修复': '#c03535',
}
export const FALLBACK_COLORS = ['#3d6fa8', '#3e6b52', '#7a5fa8', '#a8722e', '#c03535', '#4e8a8a']

export function tagColor(tag: string): string {
  if (TAG_COLORS[tag]) return TAG_COLORS[tag]
  let hash = 0
  for (let i = 0; i < tag.length; i++) hash = ((hash << 5) - hash) + tag.charCodeAt(i)
  return FALLBACK_COLORS[Math.abs(hash) % FALLBACK_COLORS.length]
}
