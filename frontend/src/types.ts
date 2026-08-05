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

export interface AppConfig {
  blender_path: string | null
  blender_version: string | null
  blender_versions: string | null
  deploy_server_url: string | null
  deploy_token: string | null
  auto_connect: boolean
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
}

export interface Timeline {
  project: string
  entries: TimelineEntry[]
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