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

export interface TimelineEntry {
  id: string
  timestamp: string
  author: string
  message: string
  files: string[]
  file_status: string[]
  preview: string
  tag?: string | null
  milestone?: boolean
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