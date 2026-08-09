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
  import ConfirmDialog from './lib/ConfirmDialog.svelte'
  import CloseBehaviorDialog from './lib/CloseBehaviorDialog.svelte'
  import InputDialog from './lib/InputDialog.svelte'
  import SettingsDialog from './lib/SettingsDialog.svelte'
  import ConflictDialog from './lib/ConflictDialog.svelte'
  import Toast from './lib/Toast.svelte'
  import type { RecentEntry, TimelineEntry, ProjectType, CommitProgress, RollbackState, RollbackChoice } from './types'
  import * as api from './api'
  import { openFolder } from './api'
  import { open } from '@tauri-apps/plugin-dialog'
  import { parseStatus } from './lib/statusParse'
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
  let statusError = $state(false)
  let trackedFiles = $state<string[]>([])
  let statusText = $state('Ready')
  let toastId = $state(0)
  let toasts = $state<{ id: number; msg: string; type: string }[]>([])
  let showCommit = $state(false)
  let showSettings = $state(false)
  let showCloseAsk = $state(false)
  let pendingRemove = $state<string | null>(null)
  let pendingExport = $state<{ parentDir: string; defaultName: string; entryId: string } | null>(null)
  let connected = $state(false)
  let accessToken = $state('')
  let siteName = $state('')
  let siteLocked = $state(false)
  let deployStatus = $state<{ local: number; remote: number | null } | null>(null)
  let commitSelection = $state<string[] | null>(null)
  let rollbackState = $state<RollbackState>({ rollback_point: null, head_matches: false })
  let rollbackBusy = $state(false)
  let conflictFiles = $state<string[] | null>(null)
  let resolvingConflict = $state(false)
  let deploying = $state(false)
  let recording = $state(false)
  let loadSeq = 0
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
      const files = await api.getTrackedFiles(path)
      if (path !== activePath) return
      trackedFiles = files
    } catch {
      if (path === activePath) trackedFiles = []
    }
  }

  async function refreshStatus() {
    if (projectType !== 'initialized') return
    const path = activePath
    try {
      const s = await api.status(path)
      if (path !== activePath) return
      statusOutput = s.output
      statusError = false
    } catch {
      if (path === activePath) {
        statusOutput = ''
        statusError = true
      }
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
    const path = activePath
    if (!path) { deployStatus = null; return }
    try {
      const serverUrl = await api.getDeployServerUrl()
      if (path !== activePath) return
      if (!serverUrl) { deployStatus = null; return }
      const token = await api.getDeployToken()
      if (path !== activePath) return
      deployStatus = await api.getDeployStatus(serverUrl, token || null, path)
    } catch {
      if (path === activePath) deployStatus = null
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

  async function loadProject(path: string, preserveSelection = false) {
    const seq = ++loadSeq
    activePath = normalizePath(path)
    if (!preserveSelection) commitSelection = null
    rollbackState = { rollback_point: null, head_matches: false }
    try {
      const pt = await api.detect(path)
      if (seq !== loadSeq) return
      projectType = pt
      if (pt === 'initialized') {
        const [timeline, cfg, autoConn] = await Promise.all([api.getTimeline(path), api.readConfig(path), api.getAutoConnect()])
        if (seq !== loadSeq) return
        entries = timeline.entries.toReversed()
        accessToken = cfg.access_token || ''
        siteName = cfg.site_name || ''
        siteLocked = cfg.site_locked
        if (seq !== loadSeq) return
        await Promise.all([refreshStatus(), loadTrackedFiles(path)])
        if (siteLocked) {
          refreshDeployStatus()
          if (autoConn) {
            testConnection()
          }
        } else {
          deployStatus = null
          connected = false
        }
      } else {
        entries = []
        statusOutput = ''
        statusError = false
        trackedFiles = []
      }
      if (seq !== loadSeq) return
      statusText = `${path.split(/[/\\]/).pop() || path} — ${pt}`
      await refreshRollbackState()
    } catch (e: any) {
      if (seq !== loadSeq) return
      statusText = `Error: ${e}`
      projectType = null
      entries = []
      statusOutput = ''
      statusError = false
      trackedFiles = []
    }
  }

  async function selectRecent(path: string) {
    if (normalizePath(path) === activePath) return
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
    if (recording) return false
    recording = true
    let unlisten: (() => void) | undefined
    try {
      statusText = '记录中...'
      unlisten = await listen<CommitProgress>('commit-progress', (event) => {
        statusText = event.payload.message
      })
      const sel = commitSelection
      let files: string[] | null = null
      if (sel !== null) {
        await refreshStatus()
        const fresh = parseStatus(statusOutput)
        const freshSet = new Set(fresh.map((f) => f.path))
        files = sel.filter((f) => freshSet.has(f))
        if (files.length === 0) {
          pushToast('所选文件已无变更', 'info')
          return false
        }
        try {
          const info = await api.getIgnoreInfo(activePath)
          const ignoredSet = new Set(info.ignored)
          const isIgnoredPath = (p: string): boolean => {
            if (ignoredSet.has(p)) return true
            const parts = p.split('/')
            for (let i = 1; i < parts.length; i++) {
              if (ignoredSet.has(parts.slice(0, i).join('/'))) return true
            }
            return false
          }
          for (const f of files) {
            if (isIgnoredPath(f)) {
              await api.removeIgnore(activePath, f)
            }
          }
          const hiddenAuto = fresh
            .filter(
              (f) =>
                (f.path === '.gitignore' || f.path === '.gitattributes') &&
                !isIgnoredPath(f.path)
            )
            .map((f) => f.path)
          if (hiddenAuto.length > 0) {
            files = [...files, ...hiddenAuto.filter((p) => !files!.includes(p))]
          }
        } catch {
          /* ignore info unavailable — fall back to raw selection */
        }
      }
      const hash = await api.record(activePath, msg, tag, noPreview, files)
      pushToast(`已记录 ${hash}`, 'success')
      await loadProject(activePath)
      if (sel !== null && files) {
        const committed = new Set(files)
        commitSelection = sel.filter((f) => !committed.has(f))
      }
      return true
    } catch (e: any) {
      pushToast(`记录失败: ${e}`, 'error')
      statusText = '记录失败'
      return false
    } finally {
      recording = false
      if (unlisten) unlisten()
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

  function handleTokenChange(val: string) {
    accessToken = val
    pushToast('Token 已保存', 'success')
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

  async function handleLocalView() {
    if (!activePath) return
    try {
      await api.openWebTimeline(activePath)
    } catch (e: any) {
      pushToast(`本地查看失败: ${e}`, 'error')
    }
  }

  async function refreshRollbackState() {
    if (!activePath || projectType !== 'initialized') return
    try {
      rollbackState = await api.getRollbackState(activePath)
    } catch { /* ignore */ }
  }

  const canRestore = $derived(rollbackState.head_matches)

  async function handleRollback(entry: TimelineEntry) {
    if (!activePath || rollbackBusy) return
    rollbackBusy = true
    try {
    await api.rollback(activePath, entry.id)
    pushToast(`已回退到记录 ${entry.id.slice(0, 7)} 之前，可重新修改后记录`, 'success')
      await loadProject(activePath)
      await refreshRollbackState()
    } catch (e: any) {
      pushToast(`回退失败: ${e}`, 'error')
    }
    rollbackBusy = false
  }

  async function handleRestore() {
    if (!activePath) return
    rollbackBusy = true
    try {
      await api.restoreRollback(activePath)
      pushToast('已恢复回退前的状态', 'success')
      await loadProject(activePath)
      await refreshRollbackState()
    } catch (e: any) {
      const msg = String(e)
      if (msg.startsWith('CONFLICT|')) {
        await loadConflictFiles()
      } else {
        pushToast(`恢复失败: ${msg}`, 'error')
      }
    }
    rollbackBusy = false
  }

  async function loadConflictFiles() {
    if (!activePath) return
    try {
      conflictFiles = await api.getRollbackConflicts(activePath)
    } catch (e: any) {
      pushToast(`读取冲突失败: ${e}`, 'error')
      conflictFiles = null
    }
  }

  async function handleResolveConflict(choices: RollbackChoice[]) {
    if (!activePath) return
    resolvingConflict = true
    try {
      await api.resolveRollbackConflicts(activePath, choices)
      pushToast('冲突已解决，已恢复', 'success')
      conflictFiles = null
      await loadProject(activePath)
      await refreshRollbackState()
    } catch (e: any) {
      pushToast(`完成恢复失败: ${e}`, 'error')
    }
    resolvingConflict = false
  }

  async function handleExportEntry(entry: TimelineEntry) {
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择导出位置',
    })
    if (!selected) return
    pendingExport = {
      parentDir: normalizePath(selected),
      defaultName: 'export_' + entry.id.slice(0, 7),
      entryId: entry.id,
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
    statusError = false
    trackedFiles = []
    statusText = 'Ready'
  }

  function handleRemoveRecent(recentPath: string) {
    pendingRemove = recentPath
  }

  async function confirmRemoveRecent() {
    const recentPath = pendingRemove
    pendingRemove = null
    if (!recentPath) return
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

  async function handleReorder(paths: string[]) {
    try {
      recentProjects = await api.reorderRecent(paths)
    } catch (e: any) {
      pushToast(`Reorder failed: ${e}`, 'error')
    }
  }

  async function confirmExport(name: string) {
    const state = pendingExport
    pendingExport = null
    if (!state) return
    const dirName = name.trim() || state.defaultName
    const dir = state.parentDir + '/' + dirName
    try {
      await api.exportEntry(activePath, state.entryId, dir)
      pushToast(`Exported to ${dir}`, 'success')
    } catch (e: any) {
      pushToast(`Export failed: ${e}`, 'error')
    }
  }

  onMount(() => {
    loadRecent()
    const closeAskUnlisten = listen('close-requested', () => { showCloseAsk = true })
    const timer = setInterval(() => {
      if (activePath && projectType === 'initialized') {
        refreshStatus()
        loadTrackedFiles(activePath)
        refreshRollbackState()
      }
    }, 3000)
    return () => {
      clearInterval(timer)
      closeAskUnlisten.then((f) => f())
    }
  })
</script>

<div class="app-layout">
  <TitleBar />

  <ProjectTabs
    entries={recentProjects}
    activePath={activePath}
    onSelect={selectRecent}
    onNewProject={handleNewProject}
    onSettings={() => showSettings = true}
    theme={theme}
    onToggleTheme={() => theme = theme === 'day' ? 'night' : 'day'}
    onRemoveRecent={handleRemoveRecent}
    onReorder={handleReorder}
  />

  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="body" onclick={() => selectedEntryId = ''}>
    <Sidebar
      {trackedFiles}
      {activePath}
      collapsed={sidebarCollapsed}
      onExpand={() => sidebarCollapsed = false}
      onCloseProject={handleRemoveRecent}
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
            <button class="path-folder" onclick={() => { try { openFolder(activePath) } catch { /* ignore */ } }} title="Open in file manager">
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
            <StatusPanel {statusOutput} {statusError} onRefresh={refreshStatus} {activePath} selection={commitSelection} onSelectionChange={(sel) => commitSelection = sel} />
          {/if}
        </div>

        {#if projectType === 'initialized'}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="tl-resize" onpointerdown={startTimelineDrag} onclick={(e) => e.stopPropagation()}></div>
          <div class="timeline-panel" style="width: {timelineWidth}px">
            <div class="tl-header">
              <span class="tl-title">时间线</span>
              <button class="tl-local-btn" onclick={handleLocalView} title="在浏览器中打开本地时间线页面">
                本地查看
              </button>
            </div>
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
              onOpenLocalView={handleLocalView}
              onMilestoneChange={() => loadProject(activePath, true)}
              rollbackPoint={rollbackState.rollback_point}
              onExportEntry={handleExportEntry}
              onRollback={handleRollback}
              rollbackBusy={rollbackBusy}
            />
          </div>
        {/if}
      </div>

      <ActionBar
        {projectType}
        onRecord={() => showCommit = true}
        onRestore={handleRestore}
        onDeploy={handleDeploy}
        {connected}
        {deployAhead}
        {deployTip}
        restoreEnabled={!!rollbackState.rollback_point && rollbackState.head_matches}
        restoreTitle={!rollbackState.rollback_point
          ? '没有可恢复的回退'
          : rollbackState.head_matches
            ? '恢复回退前的状态'
            : '回退后已有新记录，无法恢复'}
        busy={deploying || recording || rollbackBusy}
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
  <CommitDialog onConfirm={handleRecord} onCancel={() => showCommit = false} lastTag={entries[0]?.tag || ''} {canRestore} busy={recording} />
{/if}

{#if conflictFiles}
  <ConflictDialog
    conflicts={conflictFiles}
    busy={resolvingConflict}
    onResolve={handleResolveConflict}
    onCancel={() => (conflictFiles = null)}
  />
{/if}

{#if showCloseAsk}
  <CloseBehaviorDialog onCancel={() => showCloseAsk = false} />
{/if}

{#if pendingRemove}
  <ConfirmDialog
    title="移除项目"
    message={`从最近项目中移除「${pendingRemove.split(/[/\\]/).pop()}」？\n项目文件不会被删除。`}
    okLabel="移除"
    cancelLabel="取消"
    onConfirm={confirmRemoveRecent}
    onCancel={() => pendingRemove = null}
  />
{/if}

{#if pendingExport}
  <InputDialog
    title="导出到"
    label="目录名"
    initial={pendingExport.defaultName}
    okLabel="导出"
    cancelLabel="取消"
    onConfirm={confirmExport}
    onCancel={() => pendingExport = null}
  />
{/if}

{#if showSettings}
  <SettingsDialog path={activePath || ''} onClose={() => { showSettings = false; if (activePath) { loadProject(activePath); testConnection() } }} />
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
  .tl-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    flex-shrink: 0;
  }
  .tl-title {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-dim);
  }
  .tl-local-btn {
    font-size: 11px;
    padding: 3px 12px;
    border-radius: 6px;
    background: var(--surface);
    border: 1px solid var(--accent);
    color: var(--accent);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .tl-local-btn:hover {
    background: var(--accent);
    color: #fff;
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