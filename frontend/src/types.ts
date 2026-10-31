export interface MetaConfig {
  version: string
  init_time: string
  project_root: string
  web_root: string
  access_token: string | null
  project_id: string | null
  site_name: string | null
  site_locked: boolean
}

export interface PreviewFile {
  /** 源文件路径（相对项目根） */
  file: string
  /** 预览产物相对路径（相对 web_root，如 previews_glb/abc.glb） */
  path: string
  /** 预览类型：glb / image / video */
  type: string
  /** 单个预览开关 */
  enabled?: boolean
}

export interface TimelineEntry {
  id: string
  timestamp: string
  author: string
  message: string
  files: string[]
  file_status: string[]
  preview: string
  /** 预览类型：glb / image / video / 空字符串。旧数据为空时可按 preview 后缀推断 */
  preview_type?: string
  /** 一条记录内的多个预览文件；旧数据为空时按 preview/preview_type 兼容 */
  previews?: PreviewFile[]
  /** 预览失败诊断信息（例如某个文件为什么没有生成预览） */
  preview_errors?: string[]
  tag?: string | null
  /** 多标签：同一记录可多选多个二级标签 */
  tags?: string[]
  /** 流程分类：流程 id（默认 blender；旧记录可能是 modeling / painting / video） */
  tag_category?: string | null
  milestone?: boolean
  /** 记录级预览总开关（关闭后该记录所有预览不渲染；文件保留）。后端字段为 snake_case */
  preview_enabled?: boolean
  superseded?: boolean
}

export interface Timeline {
  project: string
  entries: TimelineEntry[]
  rollback_marker?: { rollback_point: string; head_before: string } | null
}

export interface RecentEntry {
  path: string
  last_access: string
}

export interface StatusResult {
  clean: boolean
  output: string
}

export interface LogEntry {
  hash: string
  date: string
  message: string
}

export type ProjectType = 'fresh' | 'existing' | 'initialized'

export interface CommitProgress {
  phase: string
  message: string
}

export interface RollbackState {
  /** 灰显记录（当前选中回退的 X；仅 X 灰显不可回退） */
  rollback_point: string | null
  /** 恢复可用性：当前 HEAD == X-1（回退后 HEAD） */
  head_matches: boolean
  /** X 删除的文件（标记"回退删除"，记录时算删除） */
  deleted_files: string[]
  /** X 修改/新增的文件（检出为未提交"回退改动"；内容与检出一致 = "已检出"，不符 = "已修改"） */
  change_files: string[]
  /** 检出文件中被"真修改"的（内容与检出 hash 不符；标签显示"已修改"，其余显示"已检出"） */
  modified_files: string[]
}

export type RollbackChoice = [string, 'current' | 'stashed']