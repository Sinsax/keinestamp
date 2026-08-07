import { invoke } from '@tauri-apps/api/core'
import type { MetaConfig, Timeline, RecentEntry, StatusResult, LogEntry, ProjectType } from './types'

export async function detect(path: string): Promise<ProjectType> {
  return invoke<ProjectType>('detect', { path })
}

export async function initProject(path: string): Promise<MetaConfig> {
  return invoke<MetaConfig>('init_project', { path })
}

export async function record(path: string, message: string, tag?: string | null, noPreview?: boolean, files?: string[] | null): Promise<string> {
  return invoke<string>('record', { path, message, tag: tag ?? null, noPreview: noPreview ?? false, files: files ?? null })
}

export async function push(path: string): Promise<void> {
  return invoke<void>('push', { path })
}

export async function deploy(path: string): Promise<void> {
  return invoke<void>('deploy', { path })
}

export async function testConnection(serverUrl: string, deployToken?: string | null): Promise<boolean> {
  return invoke<boolean>('test_connection', { serverUrl, deployToken: deployToken ?? null })
}

export async function getDeployToken(): Promise<string> {
  return invoke<string>('get_deploy_token')
}

export async function setDeployToken(token: string): Promise<void> {
  return invoke<void>('set_deploy_token', { token })
}

export async function pull(path: string): Promise<void> {
  return invoke<void>('pull', { path })
}

export async function hasRemote(path: string): Promise<boolean> {
  return invoke<boolean>('has_remote', { path })
}

export async function revert(path: string): Promise<void> {
  return invoke<void>('revert', { path })
}

export async function status(path: string): Promise<StatusResult> {
  return invoke<StatusResult>('status', { path })
}

export async function log(path: string, count?: number): Promise<LogEntry[]> {
  return invoke<LogEntry[]>('log', { path, count: count ?? null })
}

export async function readConfig(path: string): Promise<MetaConfig> {
  return invoke<MetaConfig>('read_config', { path })
}

export async function updateConfig(path: string, key: string, value: string | null): Promise<void> {
  return invoke<void>('update_config', { path, key, value })
}

export async function exportEntry(path: string, entryId: string, outputDir: string): Promise<void> {
  return invoke<void>('export_entry', { path, entryId, outputDir })
}

export async function getTimeline(path: string): Promise<Timeline> {
  return invoke<Timeline>('get_timeline', { path })
}

export async function getRecent(): Promise<RecentEntry[]> {
  return invoke<RecentEntry[]>('get_recent')
}

export async function addRecent(path: string): Promise<void> {
  return invoke<void>('add_recent', { path })
}

export async function removeRecent(path: string): Promise<void> {
  return invoke<void>('remove_recent', { path })
}

export async function openFolder(path: string): Promise<void> {
  return invoke<void>('open_folder', { path })
}

export async function deleteEntry(path: string, entryId: string): Promise<void> {
  return invoke<void>('delete_entry', { path, entryId })
}

export async function getProjectLog(path: string, count?: number): Promise<string[]> {
  return invoke<string[]>('get_project_log', { path, count: count ?? null })
}

export async function getTrackedFiles(path: string): Promise<string[]> {
  return invoke<string[]>('get_tracked_files', { path })
}

export async function getFileDiff(path: string, entryId: string, filePath: string): Promise<string> {
  return invoke<string>('get_file_diff', { path, entryId, filePath })
}

export async function getImagePreview(path: string, entryId: string, filePath: string): Promise<string> {
  return invoke<string>('get_image_preview', { path, entryId, filePath })
}

export async function setEntryMilestone(path: string, entryId: string, milestone: boolean): Promise<void> {
  return invoke<void>('set_entry_milestone', { path, entryId, milestone })
}

export async function getGlbPreview(path: string, entryId: string, filePath: string): Promise<string> {
  return invoke<string>('get_glb_preview', { path, entryId, filePath })
}

export async function downloadBlender(version?: string | null): Promise<string> {
  return invoke<string>('download_blender', { version: version ?? null })
}

export async function listBlenderVersions(): Promise<string[]> {
  return invoke<string[]>('list_blender_versions')
}

export async function listInstalledBlenderVersions(): Promise<string[]> {
  return invoke<string[]>('list_installed_blender_versions')
}

export async function getBlenderPath(): Promise<string> {
  return invoke<string>('get_blender_path')
}

export async function setBlenderPath(path: string): Promise<void> {
  return invoke<void>('set_blender_path', { path })
}

export async function openWebTimeline(path: string): Promise<void> {
  return invoke<void>('open_web_timeline', { path })
}

export async function openSite(url: string): Promise<void> {
  return invoke<void>('open_site', { url })
}

export async function getDeployServerUrl(): Promise<string> {
  return invoke<string>('get_deploy_server_url')
}

export async function setDeployServerUrl(url: string): Promise<void> {
  return invoke<void>('set_deploy_server_url', { url })
}

export async function getAutoConnect(): Promise<boolean> {
  return invoke<boolean>('get_auto_connect')
}

export async function setAutoConnect(enabled: boolean): Promise<void> {
  return invoke<void>('set_auto_connect', { enabled })
}

export async function getDeployStatus(serverUrl: string, deployToken: string | null, path: string): Promise<{ local: number; remote: number | null }> {
  return invoke('get_deploy_status', { serverUrl, deployToken, path })
}
