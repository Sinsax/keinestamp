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
  import { open, ask } from '@tauri-apps/plugin-dialog'
  import { listen } from '@tauri-apps/api/event'

  function cleanPath(p: string): string {
    return p.replace(/^\\\\\?\\/, '')
  }

  function normalizePath(p: string): string {
    return cleanPath(p)
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
  let connected = $state(false)
  let accessToken = $state('')
  let siteName = $state('')
  let siteLocked = $state(false)
  let deployStatus = $state<{ local: number; remote: number | null } | null>(null)
  let commitSelection = $state<string[] | null>(null)
  let canRestore = $state(false)
  let deploying = $state(false)
  let theme = $state<'day' | 'night'>(localStorage.getItem('keinestamp_theme') === 'night' ? 'night' : 'day')

  $effect(() => {
    document.documentElement.dataset.theme = theme
    localStorage.setItem('keinestamp_theme', theme)
  })

  let sidebarCollapsed = $state(localStorage.getItem('keinestamp_sidebar_collapsed') === '1')
  let timelineWidth = $state(Math.min(480, Math.max(240, parseInt(localStorage.getItem('keinestamp_timeline_width') || '320', 10) || 320)))

  $effect(() => {
    localStorage.setItem('keinestamp_sidebar_collapsed', sidebarCollapsed ? '1' : '0')
  })
  $effect(() => {
    localStorage.setItem('keinestamp_timeline_width', String(timelineWidth))
  })

  function startTimelineDrag(e: PointerEvent) {
    e.preventDefault()
    const startX = e.clientX
    const startW = timelineWidth
    const onMove = (ev: PointerEvent) => {
      const w = startW - (ev.clientX - startX)
      timelineWidth = Math.min(480, Math.max(240, w))
    }
    const onUp = () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      window.removeEventListener('pointercancel', onUp)
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    window.addEventListener('pointercancel', onUp)
  }

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

  async function testConnection() {
    try {
      const [serverUrl, deployToken] = await Promise.all([api.getDeployServerUrl(), api.getDeployToken()])
      if (!serverUrl) { connected = false; return }
      connected = await api.testConnection(serverUrl, deployToken)
      if (connected) refreshDeployStatus()
    } catch {
      connected = false
    }
  }

  async function refreshDeployStatus() {
    if (!activePath) { deployStatus = null; return }
    try {
      const serverUrl = await api.getDeployServerUrl()
      if (!serverUrl) { deployStatus = null; return }
      const token = await api.getDeployToken()
      deployStatus = await api.getDeployStatus(serverUrl, token || null, activePath)
    } catch {
      deployStatus = null
    }
  }

  let deployAhead = $derived(!!deployStatus && deployStatus.local > (deployStatus.remote ?? 0))
  let deployTip = $derived.by(() => {
    if (!deployStatus) return '部署到服务器'
    const r = deployStatus.remote ?? '—'
    return deployAhead
      ? `有新版本可部署：本地 v${deployStatus.local} > 服务器 v${r}`
      : `已是最新：本地 v${deployStatus.local} / 服务器 v${r}`
  })

  async function loadProject(path: string) {
    activePath = normalizePath(path)
    commitSelection = null
    try {
      const pt = await api.detect(path)
      projectType = pt
      if (pt === 'initialized') {
        const [timeline, cfg, autoConn] = await Promise.all([api.getTimeline(path), api.readConfig(path), api.getAutoConnect()])
        entries = timeline.entries.toReversed()
        hasRemote = await api.hasRemote(path)
        accessToken = cfg.access_token || ''
        siteName = cfg.site_name || ''
        siteLocked = cfg.site_locked
        canRestore = await api.canRestoreRevert(path)
        await Promise.all([refreshStatus(), loadTrackedFiles(path), refreshDeployStatus()])
        if (autoConn) {
          testConnection()
        }
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
      const normPath = normalizePath(selected)
      statusText = `Initializing ${normPath}...`
      await api.initProject(normPath)
      pushToast('Project initialized', 'success')
      await loadRecent()
      await loadProject(selected)
    } catch (e: any) {
      pushToast(`Init failed: ${e}`, 'error')
      statusText = 'Init failed'
    }
  }

  async function handleRecord(msg: string, tag: string | null, noPreview: boolean = false): Promise<boolean> {
    statusText = '记录中...'
    const unlisten = await listen<CommitProgress>('commit-progress', (event) => {
      statusText = event.payload.message
    })
    try {
      const hash = await api.record(activePath, msg, tag, noPreview, commitSelection)
      pushToast(`已记录 ${hash}`, 'success')
      commitSelection = null
      await loadProject(activePath)
      return true
    } catch (e: any) {
      pushToast(`记录失败: ${e}`, 'error')
      statusText = '记录失败'
      return false
    } finally {
      unlisten()
    }
  }

  async function handleDeploy() {
    if (!activePath) return
    deploying = true
    statusText = '部署中...'
    try {
      await api.deploy(activePath)
      pushToast('部署成功', 'success')
      await testConnection()
      await loadProject(activePath)
      statusText = '部署完成'
    } catch (e: any) {
      pushToast(`部署失败: ${e}`, 'error')
      statusText = '部署失败'
    } finally {
      deploying = false
    }
  }

  async function handleTokenChange(val: string) {
    try {
      await api.updateConfig(activePath, 'access_token', val || null)
      accessToken = val
      pushToast('Token 已保存', 'success')
    } catch (e: any) {
      pushToast(`Token 保存失败: ${e}`, 'error')
    }
  }

  async function handleSiteNameChange(val: string): Promise<boolean> {
    if (!/^[A-Za-z0-9_-]+$/.test(val)) {
      pushToast('网页名称仅限英文、数字、- 和 _', 'error')
      return false
    }
    try {
      await api.updateConfig(activePath, 'site_name', val || null)
      siteName = val
      pushToast('网页名称已保存', 'success')
      return true
    } catch (e: any) {
      pushToast(`网页名称保存失败: ${e}`, 'error')
      return false
    }
  }

  async function handleOpenSite() {
    if (!siteName) {
      pushToast('未设置网页名称', 'info')
      return
    }
    try {
      const serverUrl = await api.getDeployServerUrl()
      if (!serverUrl) {
        pushToast('未配置服务器地址', 'info')
        return
      }
      const url = `${serverUrl.replace(/\/+$/, '')}/${siteName}/`
      await api.openSite(url)
    } catch (e: any) {
      pushToast(`打开网页失败: ${e}`, 'error')
    }
  }

  async function handleRevert() {
    statusText = 'Reverting...'
    try {
      await api.revert(activePath)
      pushToast('已撤回上次记录（文件保留在工作区）', 'success')
      await loadProject(activePath)
    } catch (e: any) {
      pushToast(`Revert: ${e}`, 'info')
      statusText = 'Revert skipped'
    }
  }

  async function handleRestore() {
    statusText = '恢复中...'
    try {
      await api.restoreRevert(activePath)
      pushToast('已恢复上次撤回的记录', 'success')
      await loadProject(activePath)
    } catch (e: any) {
      pushToast(`恢复失败: ${e}`, 'error')
      statusText = '恢复失败'
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
    const name = recentPath.split(/[/\\]/).pop()
    const ok = await ask(`从最近项目中移除「${name}」？\n项目文件不会被删除。`, {
      title: '移除项目',
      kind: 'warning',
      okLabel: '移除',
      cancelLabel: '取消',
    })
    if (!ok) return
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

<div class="app-layout">
  <TitleBar />

  <ProjectTabs
    entries={recentProjects.toReversed()}
    activePath={activePath}
    onSelect={selectRecent}
    onNewProject={handleNewProject}
    onSettings={() => showSettings = true}
    theme={theme}
    onToggleTheme={() => theme = theme === 'day' ? 'night' : 'day'}
    onRemoveRecent={handleRemoveRecent}
  />

  <div class="body" onclick={() => selectedEntryId = ''}>
    <Sidebar
      {trackedFiles}
      {activePath}
      collapsed={sidebarCollapsed}
      onExpand={() => sidebarCollapsed = false}
      onCloseProject={handleCloseAndRemove}
    />

    <div class="main-area">
      <div class="toolbar">
        <button
          class="toolbar-icon"
          onclick={() => sidebarCollapsed = !sidebarCollapsed}
          title={sidebarCollapsed ? '展开侧栏' : '折叠侧栏'}
        >
          {#if sidebarCollapsed}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M6 3l5 5-5 5V3z"/></svg>
          {:else}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M3 6l5 5 5-5H3z"/></svg>
          {/if}
        </button>
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
              <div>从上方标签页选择一个项目</div>
              <div style="font-size:12px">或点击 + 创建新项目</div>
            </div>
          {:else if projectType !== 'initialized'}
            <div class="empty-state">
              <div class="big"><svg viewBox="0 0 24 24" width="32" height="32" fill="currentColor" opacity="0.5"><path d="M12 2a1 1 0 01.894.553l9 18A1 1 0 0121 22H3a1 1 0 01-.894-1.447l9-18A1 1 0 0112 2zm0 3.236L4.472 20h15.056L12 5.236z"/><path d="M12 9a.5.5 0 01.5.5v5a.5.5 0 01-1 0v-5A.5.5 0 0112 9z"/><circle cx="12" cy="16.5" r=".5"/></svg></div>
              <div>未初始化</div>
              <div style="font-size:12px">请重新打开文件夹进行初始化</div>
            </div>
          {:else}
            <StatusPanel {statusOutput} onRefresh={refreshStatus} selection={commitSelection} onSelectionChange={(sel) => commitSelection = sel} />
          {/if}
        </div>

        {#if projectType === 'initialized'}
          <div class="tl-resize" onpointerdown={startTimelineDrag} onclick={(e) => e.stopPropagation()}></div>
          <div class="timeline-panel" style="width: {timelineWidth}px">
            <EntryList
              entries={entries}
              selectedId={selectedEntryId}
              activePath={activePath}
              onSelect={(id) => selectedEntryId = id}
              onDblClick={(e) => detailEntry = e}
              accessToken={accessToken}
              onTokenChange={handleTokenChange}
              siteName={siteName}
              siteLocked={siteLocked}
              onSiteNameChange={handleSiteNameChange}
              onOpenSite={handleOpenSite}
              onMilestoneChange={() => loadProject(activePath)}
            />
          </div>
        {/if}
      </div>

      <ActionBar
        {projectType}
        onRecord={() => showCommit = true}
        onExport={handleExport}
        onRevert={handleRevert}
        onRestore={handleRestore}
        onDeploy={handleDeploy}
        {connected}
        {deployAhead}
        {deployTip}
        {canRestore}
      />

      <StatusBar
        text={statusText}
        {connected}
      />
    </div>
  </div>
</div>

<DetailPanel entry={detailEntry} activePath={activePath} onClose={() => detailEntry = null} />

{#if showCommit}
  <CommitDialog onConfirm={handleRecord} onCancel={() => showCommit = false} lastTag={entries[0]?.tag || ''} {canRestore} />
{/if}

{#if showSettings}
  <SettingsDialog path={activePath || ''} onClose={() => { showSettings = false; cachedVersions = []; if (activePath) { loadProject(activePath); testConnection() } }} cachedVersions={cachedVersions} />
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
    border-left: 1px solid var(--border);
    background: var(--bg);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    transition: none;
  }
  .toolbar-icon {
    background: none;
    border: none;
    color: var(--text-dim);
    padding: 4px 6px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: background 0.12s, color 0.12s;
  }
  .toolbar-icon:hover {
    background: var(--surface2);
    color: var(--accent);
  }
  .tl-resize {
    width: 5px;
    cursor: col-resize;
    flex-shrink: 0;
    background: transparent;
    transition: background 0.15s var(--ease);
    z-index: 1;
  }
  .tl-resize:hover {
    background: var(--accent-soft);
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
    color: var(--accent);
    background: var(--accent-soft);
  }
</style>