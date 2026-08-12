<script lang="ts">
  import { onMount } from 'svelte'
  import TitleBar from './lib/TitleBar.svelte'
  import Sidebar from './lib/Sidebar.svelte'
  import ProjectTabs from './lib/ProjectTabs.svelte'
  import EntryList from './lib/EntryList.svelte'
  import StatusPanel from './lib/StatusPanel.svelte'
  import ActionBar from './lib/ActionBar.svelte'
  import StatusBar from './lib/StatusBar.svelte'
  import CommitDialog from './lib/CommitDialog.svelte'
  import RollbackConflictDialog from './lib/RollbackConflictDialog.svelte'
  import ConfirmDialog from './lib/ConfirmDialog.svelte'
  import CloseBehaviorDialog from './lib/CloseBehaviorDialog.svelte'
  import InputDialog from './lib/InputDialog.svelte'
  import SettingsDialog from './lib/SettingsDialog.svelte'
  import BusyOverlay from './lib/BusyOverlay.svelte'
  import Toast from './lib/Toast.svelte'
  import type { RecentEntry, TimelineEntry, ProjectType, CommitProgress, RollbackState, RollbackChoice } from './types'

interface BlenderDownloadProgress {
  phase: string
  message: string
  percent: number
  bytes_downloaded: number
  bytes_total: number
}
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
  let autoRestoreProject = $state(false)
  let projectType = $state<ProjectType | null>(null)
  let entries = $state<TimelineEntry[]>([])
  let selectedEntryId = $state('')
  let statusOutput = $state('')
  let statusError = $state(false)
let statusErrorMsg = $state('')
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
  let rollbackState = $state<RollbackState>({ rollback_point: null, head_matches: false, deleted_files: [], change_files: [], modified_files: [] })
  // 回退版本改动标注：change_files（回退改动，可勾选）+ deleted_files（回退删除，只读）
const rollbackChangeFiles = $derived(rollbackState.change_files)
const rollbackDeletedFiles = $derived(rollbackState.deleted_files)
const rollbackModifiedFiles = $derived(rollbackState.modified_files)
  let rollbackBusy = $state(false)
  let pendingReinit = $state<string | null>(null)
  // 统一确认弹窗：回退丢弃 / 恢复丢弃（丢弃=执行，留下=取消不操作）
  let pendingConfirm = $state<{ kind: 'rollback' | 'restore'; entry?: TimelineEntry; title: string; message: string } | null>(null)
  let deploying = $state(false)
  let backupBusy = $state(false)
  let recording = $state(false)
  let busyText = $state('')
  let commitProgressMsg = $state('')
  let blenderDlActive = $state(false)
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
    if (type === 'error') console.log(`[debug] toast error:`, msg)
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
      statusErrorMsg = ''
    } catch (e: any) {
      if (path === activePath) {
        statusOutput = ''
        statusError = true
        statusErrorMsg = String(e)
        console.error('[refreshStatus] error:', e)
      }
    }
  }

  async function testConnection() {
    try {
      // 服务开关门控：网页部署关闭 → 不探测（connected=false）；autoConn/设置关闭/部署后统一走此判断
      const [deployOn] = await api.getServerToggles()
      if (!deployOn) { connected = false; return }
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
    const t0 = performance.now()
    const seq = ++loadSeq
    activePath = normalizePath(path)
    // 刷新常用仓库/标签栏（clone 下载等新项目路径）
    loadRecent()
    // 自动恢复上次项目：记录最后一次打开的项目
    try { localStorage.setItem('keinestamp_last_project', activePath) } catch { /* ignore */ }
    if (!preserveSelection) commitSelection = null
    rollbackState = { rollback_point: null, head_matches: false, deleted_files: [], change_files: [], modified_files: [] }
    try {
      const t1 = performance.now()
      const pt = await api.detect(path)
      if (seq !== loadSeq) return
      projectType = pt
      if (pt === 'initialized') {
        const t2 = performance.now()
        const [timeline, cfg, autoConn] = await Promise.all([api.getTimeline(path), api.readConfig(path), api.getAutoConnect()])
        if (seq !== loadSeq) return
        entries = timeline.entries.toReversed()
        accessToken = cfg.access_token || ''
        siteName = cfg.site_name || ''
        siteLocked = cfg.site_locked
        // 等待全部刷新完成（status/变更面板/回退状态）——loadProject 返回时 UI 已到位，
        // 调用方（回退/恢复等）在此时才提示完成，保证「提示与 UI 变化同步」
        if (seq === loadSeq) {
          await Promise.all([refreshStatus(), loadTrackedFiles(path), refreshRollbackState()])
        }
        // 服务器连接独立于部署状态：autoConn 时始终探测（未部署项目也能首次部署）
        if (autoConn) {
          testConnection()
        } else {
          connected = false
        }
        if (siteLocked) {
          refreshDeployStatus()
        } else {
          deployStatus = null
        }
        if (seq === loadSeq) {
          console.log(`[loadProject] detect=${(t1 - t0).toFixed(0)}ms meta=${(performance.now() - t2).toFixed(0)}ms total=${(performance.now() - t0).toFixed(0)}ms`)
        }
      } else if (pt === 'existing') {
        // 配置缺失（.keinestamp.toml 不存在）：报错提示 + 引导重新生成
        entries = []
        statusOutput = ''
        statusError = false
        trackedFiles = []
        pendingReinit = path
        pushToast('项目配置缺失（.keinestamp.toml）：需要重新生成', 'error')
      } else {
        entries = []
        statusOutput = ''
        statusError = false
        trackedFiles = []
      }
      if (seq !== loadSeq) return
      statusText = `${path.split(/[/\\]/).pop() || path} — ${pt}`
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

  // .keinestamp.toml 缺失（existing 项目）→ 引导重新生成（init 补洞/建仓）
  async function reinitProject() {
    const path = pendingReinit
    pendingReinit = null
    if (!path) return
    busyText = '重新生成配置...'
    try {
      await api.initProject(path)
      pushToast('项目配置已重新生成', 'success')
      await loadRecent()
      await loadProject(path)
    } catch (e: any) {
      pushToast(`重新生成失败: ${e}`, 'error')
    }
    busyText = ''
  }

  async function handleRecord(msg: string, tag: string | null, noPreview: boolean = false): Promise<boolean> {
    if (recording) return false
    recording = true
    let unlisten: (() => void) | undefined
    try {
      statusText = '记录中...'
      unlisten = await listen<CommitProgress>('commit-progress', (event) => {
        statusText = event.payload.message
        commitProgressMsg = event.payload.message
      })
      const sel = commitSelection
      let files: string[] | null = null
      if (sel !== null) {
        await refreshStatus()
        const fresh = parseStatus(statusOutput)
        const freshSet = new Set(fresh.map((f) => f.path))
        // 目录路径不提交（后端 stage 目录 = 目录下全部文件）：勾选 UI 的目录残留会全提
        files = sel.filter((f) => !f.endsWith('/') && freshSet.has(f))
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
      commitProgressMsg = ''
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

  async function handleBackup() {
    if (!activePath) return
    backupBusy = true
    statusText = '备份中...'
    try {
      await api.backup(activePath)
      pushToast('备份成功', 'success')
      statusText = '备份完成'
    } catch (e: any) {
      const msg = String(e)
      if (msg.includes('离线创建')) {
        // 离线仓库无法备份 → 引导迁移为在线仓库
        pendingMigrate = activePath
      } else {
        pushToast(`备份失败: ${msg}`, 'error')
      }
      statusText = '备份失败'
    } finally {
      backupBusy = false
    }
  }

  async function confirmMigrateAndBackup() {
    const path = pendingMigrate
    pendingMigrate = null
    if (!path) return
    backupBusy = true
    busyText = '迁移为在线仓库...'
    try {
      await api.migrateRepo(path)
      pushToast('已迁移为在线仓库，正在备份...', 'success')
      await api.backup(path)
      pushToast('备份成功', 'success')
      await loadProject(path)
    } catch (e: any) {
      pushToast(`迁移/备份失败: ${e}`, 'error')
    }
    busyText = ''
    backupBusy = false
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
    } catch (e: any) {
      console.error('[rollbackState] error:', e)
    }
  }

  async function handleRollback(entry: TimelineEntry) {    if (!activePath || rollbackBusy) return
    rollbackBusy = true
    busyText = '回退中...'
    try {
      await api.rollback(activePath, entry.id)
      // 等 UI 全部刷新完成（loadProject 内含 status/变更面板/回退状态 await）→
      // 清遮罩 + toast 与 UI 变化同时呈现；回退后默认全部取消选择（用户手动勾选要提交的）
      await loadProject(activePath)
      commitSelection = []
      busyText = ''
      rollbackBusy = false
      pushToast(`已回退到记录 ${entry.id.slice(0, 7)} 之前（文件保留该记录的版本，可修改后记录）`, 'success')
      return
    } catch (e: any) {
      const msg = String(e)
      if (msg.includes('需要丢弃确认')) {
        // 连续回退：回退期间的更改内容（上次回退检出的中间态）需丢弃确认
        const files = rollbackState.modified_files?.length ? `\n文件：${rollbackState.modified_files.join(', ')}` : ''
        pendingConfirm = {
          kind: 'rollback',
          entry,
          title: '丢弃并继续回退',
          message: `检测到回退期间的更改内容（上次回退检出的文件），继续回退将丢弃这些更改，且恢复时不会找回。${files}\n确定丢弃并回退到记录 ${entry.id.slice(0, 7)} 之前吗？`,
        }
      } else {
        pushToast(`回退失败: ${msg}`, 'error')
      }
    }
    busyText = ''
    rollbackBusy = false
  }

  async function confirmDiscardRollback() {
    const entry = pendingConfirm?.kind === 'rollback' ? pendingConfirm.entry : undefined
    pendingConfirm = null
    if (!entry || !activePath) return
    rollbackBusy = true
    busyText = '回退中...（丢弃中间更改）'
    try {
      await api.rollback(activePath, entry.id, true)
      await loadProject(activePath)
      commitSelection = []
      busyText = ''
      rollbackBusy = false
      pushToast(`已丢弃回退期间的更改并回退到记录 ${entry.id.slice(0, 7)} 之前`, 'success')
      return
    } catch (e: any) {
      pushToast(`回退失败: ${e}`, 'error')
    }
    busyText = ''
    rollbackBusy = false
  }

  async function handleRestore() {
    if (!activePath) return
    // 恢复确认：只有"真修改"（用户改过/删过内容）才弹窗；回退删除标记属检出，直接恢复
    const hasChanges = (rollbackState.modified_files?.length ?? 0) > 0
    if (hasChanges) {
      const files = rollbackState.modified_files?.length ? `\n文件：${rollbackState.modified_files.join(', ')}` : ''
      pendingConfirm = {
        kind: 'restore',
        title: '恢复前确认',
        message: `回退期间有已修改的内容：${files}\n「丢弃并恢复」将丢弃这些更改并恢复回退前的状态；\n「留下」将取消恢复（保持当前回退状态）。`,
      }
      return
    }
    await doRestore(null)
  }

  async function doRestore(confirm: boolean | null) {
    if (!activePath) return
    rollbackBusy = true
    busyText = '恢复中...'
    try {
      await api.restoreRollback(activePath, confirm)
      // 等 UI 全部刷新完成 → 清遮罩 + toast 与 UI 变化同时呈现（恢复后保持默认全选）
      await loadProject(activePath)
      busyText = ''
      rollbackBusy = false
      pushToast('已恢复回退前的状态', 'success')
      return
    } catch (e: any) {
      const msg = String(e)
      if (msg.includes('回退冲突')) {
        await loadConflictFiles()
      } else if (msg.includes('恢复需要确认')) {
        // 快照过时/后端实时判定有真修改 → 弹确认窗（后端为权威判定源）
        const files = rollbackState.modified_files?.length ? `\n文件：${rollbackState.modified_files.join(', ')}` : ''
        pendingConfirm = {
          kind: 'restore',
          title: '恢复前确认',
          message: `回退期间有已修改的内容：${files}\n「丢弃并恢复」将丢弃这些更改并恢复回退前的状态；\n「留下」将取消恢复（保持当前回退状态）。`,
        }
      } else {
        pushToast(`恢复失败: ${msg}`, 'error')
      }
    }
    busyText = ''
    rollbackBusy = false
  }

  // 重建 web 产物（暂移除：web 由记录时自动生成）

  let conflictFiles = $state<string[] | null>(null)
  let resolvingConflict = $state(false)
  let pendingMigrate = $state<string | null>(null)
  let loreOk = $state(true)
  let loreError = $state('')
  let showLoreMissing = $state(false)

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
      await api.resolveRollbackConflicts(activePath, choices.map((c) => [c[0], c[1]]))
      pushToast('冲突已解决，已恢复回退前的状态', 'success')
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
    // Lore 前置依赖检测（编译期捆绑；失败 → 弹窗 + 核心操作禁用）
    ;(async () => {
      try {
        const info = await api.checkLoreEnv()
        loreOk = info.ok
        if (!info.ok) {
          loreError = 'Lore 环境异常（请重新安装/构建）'
          showLoreMissing = true
        }
      } catch (e: any) {
        loreOk = false
        loreError = String(e)
        showLoreMissing = true
      }
    })()
    ;(async () => {
      try {
        autoRestoreProject = await api.getAutoRestoreProject()
      } catch { /* ignore */ }
    })()
    // 自动恢复上次项目（设置开启且路径存在）
    ;(async () => {
      try {
        const last = localStorage.getItem('keinestamp_last_project')
        if (last && (await api.getAutoRestoreProject())) {
          const exists = await api.detect(last).catch(() => null)
          if (exists) {
            await loadProject(last)
            return
          }
        }
      } catch { /* ignore */ }
      loadRecent()
    })()
    const closeAskUnlisten = listen('close-requested', () => { showCloseAsk = true })
    // 全局 Blender 下载进度反馈：设置对话框关闭后仍可见（开始/完成/失败 toast）
    const blenderDlUnlisten = listen<BlenderDownloadProgress>('blender-download-progress', (e) => {
      const phase = e.payload.phase
      if (!blenderDlActive && (phase === 'downloading' || phase === 'extracting')) {
        blenderDlActive = true
        pushToast('正在下载 Blender...', 'info')
      } else if (blenderDlActive && phase === 'done') {
        blenderDlActive = false
        pushToast('Blender 下载完成', 'success')
      } else if (blenderDlActive && phase === 'error') {
        blenderDlActive = false
        pushToast('Blender 下载失败', 'error')
      }
    })
    const timer = setInterval(() => {
      if (activePath && projectType === 'initialized') {
        refreshStatus()
        loadTrackedFiles(activePath)
        refreshRollbackState()
      }
    }, 10000)
    return () => {
      clearInterval(timer)
      closeAskUnlisten.then((f) => f())
      blenderDlUnlisten.then((f) => f())
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
              <div>从上方标签页选择项目，或点击 + 新建项目</div>
              <button class="empty-new-btn" onclick={handleNewProject}>新建项目</button>
            </div>
          {:else if projectType !== 'initialized'}
            <div class="empty-state">
              <div class="big"><svg viewBox="0 0 24 24" width="32" height="32" fill="currentColor" opacity="0.5"><path d="M12 2a1 1 0 01.894.553l9 18A1 1 0 0121 22H3a1 1 0 01-.894-1.447l9-18A1 1 0 0112 2zm0 3.236L4.472 20h15.056L12 5.236z"/><path d="M12 9a.5.5 0 01.5.5v5a.5.5 0 01-1 0v-5A.5.5 0 0112 9z"/><circle cx="12" cy="16.5" r=".5"/></svg></div>
              <div>未初始化</div>
              <div style="font-size:12px">请重新打开文件夹进行初始化</div>
            </div>
          {:else}
            <StatusPanel {statusOutput} {statusError} {statusErrorMsg} {trackedFiles} {rollbackChangeFiles} {rollbackDeletedFiles} {rollbackModifiedFiles} onRefresh={refreshStatus} {activePath} selection={commitSelection} rollbackActive={!!rollbackState.rollback_point} onSelectionChange={(sel) => commitSelection = sel} />
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
  onBackup={handleBackup}
    {connected}
  {deployAhead}
  {deployTip}
  loreOk={loreOk}
        restoreEnabled={!!rollbackState.rollback_point && rollbackState.head_matches}
        restoreTitle={!rollbackState.rollback_point
          ? '没有可恢复的回退'
          : rollbackState.head_matches
            ? '恢复回退前的状态'
            : '回退后已有新记录，无法恢复'}
        busy={deploying || recording || rollbackBusy || backupBusy}
      />

      <StatusBar
        text={statusText}
        {connected}
      />
    </div>
  </div>
</div>

{#if showCommit}
  <CommitDialog onConfirm={handleRecord} onCancel={() => showCommit = false} lastTag={entries[0]?.tag || ''} busy={recording} progress={commitProgressMsg} />
{/if}

{#if conflictFiles}
  <RollbackConflictDialog
    conflicts={conflictFiles}
    busy={resolvingConflict}
    onResolve={handleResolveConflict}
    onCancel={() => (conflictFiles = null)}
  />
{/if}

{#if pendingConfirm}
  {#if pendingConfirm.kind === 'rollback'}
    <ConfirmDialog
      title={pendingConfirm.title}
      message={pendingConfirm.message}
      okLabel="丢弃并跳转"
      cancelLabel="留下"
      onConfirm={confirmDiscardRollback}
      onCancel={() => (pendingConfirm = null)}
    />
  {:else}
    <ConfirmDialog
      title={pendingConfirm.title}
      message={pendingConfirm.message}
      okLabel="丢弃并恢复"
      cancelLabel="留下"
      onConfirm={() => { pendingConfirm = null; doRestore(true) }}
      onCancel={() => (pendingConfirm = null)}
    />
  {/if}
{/if}

{#if pendingMigrate}
  <ConfirmDialog
    title="迁移为在线仓库"
    message={'该项目为离线创建，无法备份到服务器。\n是否迁移为在线仓库（保留全部历史，迁移后自动备份）？'}
    okLabel="迁移并备份"
    cancelLabel="取消"
    onConfirm={confirmMigrateAndBackup}
    onCancel={() => (pendingMigrate = null)}
  />
{/if}

{#if showLoreMissing}
  <ConfirmDialog
    title="缺少 Lore 环境"
    message={`${loreError || '未找到 Lore v0.8.6'}\n记录/回退/备份等操作不可用。请重新安装或构建 Keinestamp（编译期自动捆绑 Lore）。`}
    okLabel="知道了"
    cancelLabel=""
    onConfirm={() => (showLoreMissing = false)}
    onCancel={() => (showLoreMissing = false)}
  />
{/if}

{#if pendingReinit}
  <ConfirmDialog
    title="需要重新生成项目配置"
    message={`「${pendingReinit.split(/[/\\]/).pop()}」缺少 .keinestamp.toml（配置/隐藏规则缺失）。\n重新生成将补齐配置与私有目录（不影响 lore 历史）。`}
    okLabel="重新生成"
    cancelLabel="取消"
    onConfirm={reinitProject}
    onCancel={() => (pendingReinit = null)}
  />
{/if}

{#if busyText}
  <BusyOverlay text={busyText} />
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
  <SettingsDialog
    path={activePath || ''}
    recentPaths={recentProjects.map((r) => r.path)}
    onOpenProject={(p) => { showSettings = false; selectRecent(p) }}
    onClose={() => { showSettings = false; if (activePath) { loadProject(activePath); testConnection() } }} />
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