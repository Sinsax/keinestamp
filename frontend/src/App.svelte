<script lang="ts">
  import { onMount } from 'svelte'
  import TitleBar from './lib/TitleBar.svelte'
  import Sidebar from './lib/Sidebar.svelte'
  import ProjectTabs from './lib/ProjectTabs.svelte'
  import EntryList from './lib/EntryList.svelte'
  import StatusPanel from './lib/StatusPanel.svelte'
  import DetailPanel from './lib/DetailPanel.svelte'
  import ActionBar from './lib/ActionBar.svelte'
  import StatusBar from './lib/StatusBar.svelte'
  import CommitDialog from './lib/CommitDialog.svelte'
  import SettingsDialog from './lib/SettingsDialog.svelte'
  import Toast from './lib/Toast.svelte'
  import type { RecentEntry, TimelineEntry, ProjectType, CommitProgress } from './types'
  import * as api from './api'
  import { openFolder } from './api'
  import { open } from '@tauri-apps/plugin-dialog'
  import { listen } from '@tauri-apps/api/event'

  function cleanPath(p: string): string {
    return p.replace(/^\\\\\?\\/, '')
  }

  let recentProjects = $state<RecentEntry[]>([])
  let activePath = $state('')
  let projectType = $state<ProjectType | null>(null)
  let entries = $state<TimelineEntry[]>([])
  let selectedEntryId = $state('')
  let detailEntry = $state<TimelineEntry | null>(null)
  let statusOutput = $state('')
  let trackedFiles = $state<string[]>([])
  let statusText = $state('Ready')
  let toastId = $state(0)
  let toasts = $state<{ id: number; msg: string; type: string }[]>([])
  let showCommit = $state(false)
  let showSettings = $state(false)
  let cachedVersions = $state<string[]>([])
  let hasRemote = $state(false)

  async function loadCachedVersions() {
    if (cachedVersions.length === 0) {
      try {
        cachedVersions = await api.listBlenderVersions()
      } catch { /* ignore */ }
    }
  }

  function pushToast(msg: string, type: string = 'info') {
    const id = ++toastId
    toasts = [...toasts, { id, msg, type }]
    setTimeout(() => { toasts = toasts.filter((t) => t.id !== id) }, 3000)
  }

  async function loadRecent() {
    try {
      recentProjects = await api.getRecent()
    } catch (e) {
      console.error('load recent failed', e)
    }
  }

  async function loadTrackedFiles(path: string) {
    if (projectType !== 'initialized') {
      trackedFiles = []
      return
    }
    try {
      trackedFiles = await api.getTrackedFiles(path)
    } catch {
      trackedFiles = []
    }
  }

  async function refreshStatus() {
    if (projectType !== 'initialized') return
    try {
      const s = await api.status(activePath)
      statusOutput = s.output
    } catch {
      statusOutput = ''
    }
  }

  async function loadProject(path: string) {
    activePath = path
    try {
      const pt = await api.detect(path)
      projectType = pt
      if (pt === 'initialized') {
        const timeline = await api.getTimeline(path)
        entries = timeline.entries.toReversed()
        hasRemote = await api.hasRemote(path)
        await Promise.all([refreshStatus(), loadTrackedFiles(path)])
      } else {
        entries = []
        statusOutput = ''
        trackedFiles = []
      }
      statusText = `${path.split(/[/\\]/).pop() || path} — ${pt}`
    } catch (e: any) {
      statusText = `Error: ${e}`
      projectType = null
      entries = []
      statusOutput = ''
      trackedFiles = []
    }
  }

  async function selectRecent(path: string) {
    await loadProject(path)
  }

  async function handleNewProject() {
    try {
      const selected = await open({ directory: true, multiple: false, title: 'Select project folder' })
      if (!selected) return
      statusText = `Initializing ${selected}...`
      await api.initProject(selected)
      pushToast('Project initialized', 'success')
      await loadRecent()
      await loadProject(selected)
    } catch (e: any) {
      pushToast(`Init failed: ${e}`, 'error')
      statusText = 'Init failed'
    }
  }

  async function handleCommit(msg: string, tag: string | null, noPreview: boolean = false) {
    showCommit = false
    statusText = 'Committing...'
    const unlisten = await listen<CommitProgress>('commit-progress', (event) => {
      statusText = event.payload.message
    })
    try {
      const hash = await api.commit(activePath, msg, tag, noPreview)
      pushToast(`Committed ${hash}`, 'success')
      await loadProject(activePath)
    } catch (e: any) {
      pushToast(`Commit failed: ${e}`, 'error')
      statusText = 'Commit failed'
    } finally {
      unlisten()
    }
  }

  async function handleRevert() {
    statusText = 'Reverting...'
    try {
      await api.revert(activePath)
      pushToast('Last commit reverted (files preserved)', 'success')
      await loadProject(activePath)
    } catch (e: any) {
      pushToast(`Revert: ${e}`, 'info')
      statusText = 'Revert skipped'
    }
  }

  async function handleStatus() {
    try {
      const s = await api.status(activePath)
      statusOutput = s.output
      pushToast(s.output.trim() ? 'Changes detected' : 'Working tree clean', 'info')
    } catch (e: any) {
      pushToast(`Status failed: ${e}`, 'error')
    }
  }

  async function handlePush() {
    statusText = 'Pushing...'
    try {
      await api.push(activePath)
      pushToast('Pushed to remote', 'success')
      statusText = 'Push complete'
    } catch (e: any) {
      pushToast(`Push failed: ${e}`, 'error')
      statusText = 'Push failed'
    }
  }

  async function handlePull() {
    statusText = 'Pulling...'
    try {
      await api.pull(activePath)
      pushToast('Pulled from remote', 'success')
      await loadProject(activePath)
    } catch (e: any) {
      pushToast(`Pull failed: ${e}`, 'error')
      statusText = 'Pull failed'
    }
  }

  function handleCloseProject() {
    activePath = ''
    projectType = null
    entries = []
    statusOutput = ''
    trackedFiles = []
    statusText = 'Ready'
  }

  async function handleCloseAndRemove(path: string) {
    try {
      await api.removeRecent(path)
    } catch { /* ignore */ }
    recentProjects = recentProjects.filter((e) => e.path !== path)
    handleCloseProject()
  }

  async function handleRemoveRecent(recentPath: string) {
    if (!confirm(`Remove "${recentPath.split(/[/\\]/).pop()}" from recent projects?\nProject files will NOT be deleted.`)) return
    try {
      await api.removeRecent(recentPath)
      pushToast('Project removed from recent list', 'success')
      recentProjects = await api.getRecent()
      if (activePath === recentPath) {
        handleCloseProject()
      }
    } catch (e: any) {
      pushToast(`Remove failed: ${e}`, 'error')
    }
  }

  async function handleExport() {
    if (!selectedEntryId) {
      pushToast('Select an entry first', 'info')
      return
    }
    const dir = activePath + '/export_' + selectedEntryId.slice(0, 7)
    try {
      await api.exportEntry(activePath, selectedEntryId, dir)
      pushToast(`Exported to ${dir}`, 'success')
    } catch (e: any) {
      pushToast(`Export failed: ${e}`, 'error')
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (showCommit || detailEntry) return
    switch (e.key) {
      case 'c':
        if (projectType === 'initialized') showCommit = true
        break
      case 'e':
        if (projectType === 'initialized') handleExport()
        break
      case 'r':
        if (projectType === 'initialized') handleRevert()
        break
      case 's':
        if (projectType === 'initialized') handleStatus()
        break
      case 'p':
        if (projectType === 'initialized') handlePush()
        break
      case 'l':
        if (projectType === 'initialized') handlePull()
        break
    }
  }

  onMount(() => {
    loadRecent()
    loadCachedVersions()
    const timer = setInterval(() => {
      if (activePath && projectType === 'initialized') {
        refreshStatus()
        loadTrackedFiles(activePath)
      }
    }, 3000)
    return () => clearInterval(timer)
  })
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app-layout">
  <TitleBar />

  <ProjectTabs
    entries={recentProjects.toReversed()}
    activePath={activePath}
    onSelect={selectRecent}
    onNewProject={handleNewProject}
    onSettings={() => showSettings = true}
  />

  <div class="body">
    <Sidebar {trackedFiles} {activePath} onCloseProject={handleCloseAndRemove} />

    <div class="main-area">
      <div class="toolbar">
        {#if activePath}
          <span class="toolbar-path">
            <span class="path-text" title={cleanPath(activePath)}>{cleanPath(activePath)}</span>
            <button class="path-folder" onclick={() => openFolder(activePath)} title="Open in file manager">
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                <path d="M1.5 2.5a1 1 0 0 1 1-1h3.672a1 1 0 0 1 .707.293l.828.828A1 1 0 0 0 8.414 3H13.5a1 1 0 0 1 1 1v1.5H1.5V2.5Z"/>
                <path d="M1.5 6h13l-1.41 5.44a1 1 0 0 1-.97.81H3.88a1 1 0 0 1-.97-.81L1.5 6Z"/>
              </svg>
            </button>
          </span>
        {/if}
      </div>

      <div class="content-body">
        <div class="content-area">
          {#if !activePath}
            <div class="empty-state">
              <div class="big"><svg viewBox="0 0 24 24" width="32" height="32" fill="currentColor" opacity="0.3"><path d="M2 6a2 2 0 012-2h5.757a2 2 0 011.414.586l1.12 1.12A2 2 0 0013.757 6H20a2 2 0 012 2v8a2 2 0 01-2 2H4a2 2 0 01-2-2V6z"/></svg></div>
              <div>Open a project from the tabs above</div>
              <div style="font-size:12px">or create a new one with +</div>
            </div>
          {:else if projectType !== 'initialized'}
            <div class="empty-state">
              <div class="big"><svg viewBox="0 0 24 24" width="32" height="32" fill="currentColor" opacity="0.5"><path d="M12 2a1 1 0 01.894.553l9 18A1 1 0 0121 22H3a1 1 0 01-.894-1.447l9-18A1 1 0 0112 2zm0 3.236L4.472 20h15.056L12 5.236z"/><path d="M12 9a.5.5 0 01.5.5v5a.5.5 0 01-1 0v-5A.5.5 0 0112 9z"/><circle cx="12" cy="16.5" r=".5"/></svg></div>
              <div>Not initialized</div>
              <div style="font-size:12px">Press &quot;i&quot; to init</div>
            </div>
          {:else}
            <StatusPanel {statusOutput} onRefresh={refreshStatus} />
          {/if}
        </div>

        {#if projectType === 'initialized'}
          <div class="timeline-panel">
            <EntryList
              entries={entries}
              selectedId={selectedEntryId}
              activePath={activePath}
              onSelect={(id) => selectedEntryId = id}
              onDblClick={(e) => detailEntry = e}
              onPush={handlePush}
              onPull={handlePull}
              {hasRemote}
            />
          </div>
        {/if}
      </div>

      <ActionBar
        {projectType}
        onCommit={() => showCommit = true}
        onExport={handleExport}
        onRevert={handleRevert}
      />

      <StatusBar text={statusText} />
    </div>
  </div>
</div>

<DetailPanel entry={detailEntry} activePath={activePath} onClose={() => detailEntry = null} />

{#if showCommit}
  <CommitDialog onConfirm={handleCommit} onCancel={() => showCommit = false} />
{/if}

{#if showSettings}
  <SettingsDialog path={activePath || ''} onClose={() => { showSettings = false; cachedVersions = []; if (activePath) loadProject(activePath) }} cachedVersions={cachedVersions} />
{/if}

<Toast {toasts} />

<style>
  .content-body {
    display: flex;
    flex: 1;
    overflow: hidden;
  }
  .content-area {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .timeline-panel {
    width: 320px;
    border-left: 1px solid var(--border);
    background: var(--bg);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }
  .toolbar-path {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: 8px;
    flex: 1;
    overflow: hidden;
    font-size: 12px;
    color: var(--text-dim);
  }
  .path-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path-folder {
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    padding: 2px 4px;
    line-height: 1;
    border-radius: 4px;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .path-folder:hover {
    color: var(--accent, #cba6f7);
    background: rgba(203, 166, 247, 0.12);
  }
</style>