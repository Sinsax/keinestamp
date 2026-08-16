import { invoke } from '@tauri-apps/api/core'
import type { MetaConfig, Timeline, RecentEntry, StatusResult, ProjectType, RollbackState, RollbackChoice } from './types'

export async function detect(path: string): Promise<ProjectType> {
  return invoke<ProjectType>('detect', { path })
}

export async function initProject(path: string): Promise<MetaConfig> {
  return invoke<MetaConfig>('init_project', { path })
}

export async function record(path: string, message: string, tag?: string | null, tagCategory?: string | null, tags?: string[] | null, noPreview?: boolean, files?: string[] | null, previewFiles?: string[] | null): Promise<string> {
  return invoke<string>('record', { path, message, tag: tag ?? null, tagCategory: tagCategory ?? null, tags: tags ?? null, noPreview: noPreview ?? false, files: files ?? null, previewFiles: previewFiles ?? null })
}

export async function deploy(path: string): Promise<void> {
  return invoke<void>('deploy', { path })
}

export async function backup(path: string): Promise<void> {
  return invoke<void>('backup', { path })
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

export async function rollback(path: string, entryId: string, discard = false): Promise<void> {
  return invoke<void>('rollback', { path, entryId, discard })
}

export async function getRollbackState(path: string): Promise<RollbackState> {
  return invoke<RollbackState>('get_rollback_state', { path })
}

export async function restoreRollback(path: string, discardModified: boolean | null = null): Promise<void> {
  return invoke<void>('restore_rollback', { path, discardModified })
}

export async function getRollbackConflicts(path: string): Promise<string[]> {
  return invoke<string[]>('get_rollback_conflicts', { path })
}

export async function keepRollbackFile(path: string, file: string): Promise<void> {
  return invoke<void>('keep_rollback_file', { path, file })
}

export async function resolveRollbackConflicts(path: string, choices: RollbackChoice[]): Promise<void> {
  return invoke<void>('resolve_rollback_conflicts', { path, choices })
}

export async function status(path: string): Promise<StatusResult> {
  return invoke<StatusResult>('status', { path })
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

export interface TagGroup {
  id: string
  label: string
  tags: string[]
}

export async function getTagGroups(path: string): Promise<TagGroup[]> {
  return invoke<TagGroup[]>('get_tag_groups', { path })
}

export interface FileDiff {
  file: string
  diff: string
}

export async function compareEntries(path: string, entryA: string, entryB: string): Promise<FileDiff[]> {
  return invoke<FileDiff[]>('compare_entries', { path, entryA, entryB })
}

export async function getRecent(): Promise<RecentEntry[]> {
  return invoke<RecentEntry[]>('get_recent')
}

export async function removeRecent(path: string): Promise<void> {
  return invoke<void>('remove_recent', { path })
}

export async function reorderRecent(paths: string[]): Promise<RecentEntry[]> {
  return invoke<RecentEntry[]>('reorder_recent', { paths })
}

export async function openFolder(path: string): Promise<void> {
  return invoke<void>('open_folder', { path })
}

export async function addToGitignore(projectRoot: string, entry: string): Promise<void> {
  return invoke<void>('add_to_gitignore', { projectRoot, entry })
}

export interface IgnoreInfo {
  tracked: string[]
  ignored: string[]
  rule_ignored: string[]
}

export async function getIgnoreInfo(projectRoot: string, files?: string[]): Promise<IgnoreInfo> {
  return invoke<IgnoreInfo>('get_ignore_info', { projectRoot, files: files ?? [] })
}

export async function addSkipWorktree(projectRoot: string, path: string): Promise<void> {
  return invoke<void>('add_skip_worktree', { projectRoot, path })
}

export async function removeIgnore(projectRoot: string, entry: string): Promise<void> {
  return invoke<void>('remove_ignore', { projectRoot, entry })
}

export async function getTrackedFiles(path: string): Promise<string[]> {
  return invoke<string[]>('get_tracked_files', { path })
}

export async function getEmptyDirs(path: string): Promise<string[]> {
  return invoke<string[]>('get_empty_dirs', { path })
}

export async function discardFile(path: string, file: string): Promise<void> {
  return invoke<void>('discard_file', { path, file })
}

export async function setEntryMilestone(path: string, entryId: string, milestone: boolean): Promise<void> {
  return invoke<void>('set_entry_milestone', { path, entryId, milestone })
}

export async function setEntryPreview(path: string, entryId: string, enabled: boolean): Promise<void> {
  return invoke<void>('set_entry_preview', { path, entryId, enabled })
}

export async function setEntryPreviewFile(path: string, entryId: string, previewPath: string, enabled: boolean): Promise<void> {
  return invoke<void>('set_entry_preview_file', { path, entryId, previewPath, enabled })
}

export async function downloadBlender(version?: string | null): Promise<string> {
  return invoke<string>('download_blender', { version: version ?? null })
}

export async function refreshBlenderVersions(): Promise<string[]> {
  return invoke<string[]>('refresh_blender_versions')
}

export async function getCachedBlenderVersions(): Promise<string[]> {
  return invoke<string[]>('get_cached_blender_versions')
}

export async function listInstalledBlenderVersions(): Promise<string[]> {
  return invoke<string[]>('list_installed_blender_versions')
}

export async function deleteBlenderVersion(version: string): Promise<void> {
  return invoke<void>('delete_blender_version', { version })
}

export async function getCurrentBlenderVersion(): Promise<string> {
  return invoke<string>('get_current_blender_version')
}

export async function setCurrentBlenderVersion(version: string): Promise<void> {
  return invoke<void>('set_current_blender_version', { version })
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

export async function getLoreServerUrl(): Promise<string> {
  return invoke<string>('get_lore_server_url')
}

export async function setLoreServerUrl(url: string): Promise<void> {
  return invoke<void>('set_lore_server_url', { url })
}

export async function getAutoRestoreProject(): Promise<boolean> {
  return invoke<boolean>('get_auto_restore_project')
}

export async function setAutoRestoreProject(enabled: boolean): Promise<void> {
  return invoke<void>('set_auto_restore_project', { enabled })
}

export async function listServerRepos(url: string): Promise<[string, string][]> {
  return invoke<[string, string][]>('list_server_repos', { url })
}

export async function cloneServerRepo(url: string, name: string, destParent: string): Promise<string> {
  return invoke<string>('clone_server_repo', { url, name, destParent })
}

export async function deleteServerRepo(url: string, name: string): Promise<void> {
  return invoke<void>('delete_server_repo', { url, name })
}

export async function testLoreConnection(host: string): Promise<boolean> {
  return invoke<boolean>('test_lore_connection', { host })
}

export async function migrateRepo(path: string): Promise<void> {
  return invoke<void>('migrate_repo', { path })
}

export async function checkLoreEnv(): Promise<{ ok: boolean; bin: string; version: string }> {
  return invoke('check_lore_env')
}

export async function getLoreIdentity(): Promise<string> {
  return invoke<string>('get_lore_identity')
}

export async function setLoreIdentity(identity: string): Promise<void> {
  return invoke<void>('set_lore_identity', { identity })
}

export async function getAutoConnect(): Promise<boolean> {
  return invoke<boolean>('get_auto_connect')
}

export async function setAutoConnect(enabled: boolean): Promise<void> {
  return invoke<void>('set_auto_connect', { enabled })
}

export async function getServerToggles(): Promise<[boolean, boolean]> {
  return invoke<[boolean, boolean]>('get_server_toggles')
}

export async function setServerToggles(deployEnabled: boolean, loreEnabled: boolean): Promise<void> {
  return invoke<void>('set_server_toggles', { deployEnabled, loreEnabled })
}

export async function getCloseAction(): Promise<boolean | null> {
  return invoke<boolean | null>('get_close_action')
}

export async function setCloseAction(closeToTray: boolean | null): Promise<void> {
  return invoke<void>('set_close_action', { closeToTray })
}

export async function resolveCloseAction(closeToTray: boolean): Promise<void> {
  return invoke<void>('resolve_close_action', { closeToTray })
}

export async function getDeployStatus(serverUrl: string, deployToken: string | null, path: string): Promise<{ local: number; remote: number | null }> {
  return invoke('get_deploy_status', { serverUrl, deployToken, path })
}
