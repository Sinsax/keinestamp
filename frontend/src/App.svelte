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
  import { isPreviewable } from './lib/preview'
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
  // status 轮询去重：10s 定时器可能叠加（上一次 lore status 被大项目/锁拖慢时），
  // 只允许一个后台轮询在途；手动刷新（handleRecord 等 await refreshStatus）不受影响。
  let statusPollInFlight = false
  // 最新一次请求胜出：慢的旧响应不得覆盖新响应
  let statusReqSeq = 0
  let trackedFiles = $state<string[]>([])
  let emptyDirs = $state<string[]>([])
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
  // 预览标记：变更面板右键可预览文件标记；记录时只对这些文件生成预览（记录完成后清空）
  let previewMarked = $state<Set<string>>(new Set())
  let tagGroups = $state<api.TagGroup[]>([])

  function togglePreviewMark(path: string) {
    const next = new Set(previewMarked)
    if (next.has(path)) next.delete(path)
    else next.add(path)
    previewMarked = next
  }
  // 本次提交范围中是否有标记预览的文件（决定记录弹窗是否显示预览选框）
  const hasPreviewMarked = $derived(
    [...previewMarked].some((f) => isPreviewable(f) && (commitSelection === null || commitSelection.includes(f)))
  )
  let rollbackState = $state<RollbackState>({ rollback_point: null, head_matches: false, deleted_files: [], change_files: [], modified_files: [] })
  // 回退版本改动标注：change_files（回退改动，可勾选）+ deleted_files（回退删除，只读）
const rollbackChangeFiles = $derived(rollbackState.change_files)
const rollbackDeletedFiles = $derived(rollbackState.deleted_files)
const rollbackModifiedFiles = $derived(rollbackState.modified_files)
  let rollbackBusy = $state(false)
  let pendingReinit = $state<string | null>(null)
  // 统一确认弹窗：回退丢弃 / 恢复丢弃 / 回退空记录（丢弃=执行，留下=取消不操作）
  let pendingConfirm = $state<
    | { kind: 'rollback' | 'restore'; entry?: TimelineEntry; title: string; message: string }
    | {
        kind: 'empty_record'
        title: string
        message: string
        data: {
          msg: string
          tag: string | null
          tagCategory: string | null
          tags: string[]
          noPreview: boolean
          files: string[] | null
          previewFiles: string[]
        }
      }
    | null
  >(null)
  // 取消更改（git restore）：M 覆盖工作区修改有数据丢失风险 → 弹确认框
  let pendingDiscard = $state<{ file: string } | null>(null)
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

  // 全局禁用 WebView2 默认右键菜单（刷新/另存为/打印等）：组件自定义菜单已自行
  // preventDefault（条目/文件行/菜单背景），这里兜底未覆盖区域（时间线空白、文件树空白等）；
  // 输入框/文本域保留原生复制/粘贴菜单
  function globalContextMenu(e: MouseEvent) {
    const target = e.target as HTMLElement | null
    if (!target) return
    if (target.closest('input, textarea, [contenteditable="true"]')) return
    e.preventDefault()
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null
    if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return
    if (e.ctrlKey && e.key.toLowerCase() === 'r') {
      e.preventDefault()
      if (activePath && projectType === 'initialized') refreshStatus()
    } else if (e.ctrlKey && e.key === 'Enter') {
      e.preventDefault()
      if (activePath && projectType === 'initialized' && !recording && !showCommit) openCommit()
    }
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
      emptyDirs = []
      return
    }
    // 并行获取文件 + 空目录、一次性更新：避免分次到达导致文件树折叠状态竞态
    // （先到的一批触发全折叠后，晚到的空目录链会保持展开）。
    // 两个命令仍独立容错：get_empty_dirs 失败只清空 emptyDirs——
    // trackedFiles 是变更面板灰显忽略文件的数据源，不能连坐清空
    const [filesRes, dirsRes] = await Promise.all([
      api.getTrackedFiles(path).then((files) => ({ ok: true as const, files })).catch(() => ({ ok: false as const, files: [] as string[] })),
      api.getEmptyDirs(path).then((dirs) => ({ ok: true as const, dirs })).catch(() => ({ ok: false as const, dirs: [] as string[] })),
    ])
    if (path !== activePath) return
    trackedFiles = filesRes.ok ? filesRes.files : []
    emptyDirs = dirsRes.ok ? dirsRes.dirs : []
  }

  async function refreshStatus() {
    if (projectType !== 'initialized') return
    const path = activePath
    const req = ++statusReqSeq
    try {
      const s = await api.status(path)
      if (path !== activePath || req !== statusReqSeq) return
      statusOutput = s.output
      statusError = false
      statusErrorMsg = ''
    } catch (e: any) {
      if (path === activePath && req === statusReqSeq) {
        statusOutput = ''
        statusError = true
        statusErrorMsg = String(e)
        console.error('[refreshStatus] error:', e)
      }
    }
  }

  async function pollStatus() {
    if (statusPollInFlight) return
    statusPollInFlight = true
    try {
      await refreshStatus()
    } finally {
      statusPollInFlight = false
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
    const norm = normalizePath(path)
    if (norm !== activePath) {
      // 项目切换：立即清空文件列表（旧项目的树不在新项目下残留），
      // 文件树随新数据到达重建并默认全折叠（DirTree 以 activePath 变化重置）
      trackedFiles = []
      emptyDirs = []
    }
    activePath = norm
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
        const [timeline, cfg, autoConn, groups] = await Promise.all([api.getTimeline(path), api.readConfig(path), api.getAutoConnect(), api.getTagGroups(path)])
        if (seq !== loadSeq) return
        tagGroups = groups
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
        emptyDirs = []
        pendingReinit = path
        pushToast('项目配置缺失（.keinestamp.toml）：需要重新生成', 'error')
      } else {
        entries = []
        statusOutput = ''
        statusError = false
        trackedFiles = []
        emptyDirs = []
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
      emptyDirs = []
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

  function openCommit() {
    // 不再根据文件类型自动预选流程/标签——用户应自由选择
    showCommit = true
  }

  async function handleRecord(msg: string, tag: string | null, tagCategory: string | null = null, tags: string[] = [], noPreview: boolean = false): Promise<boolean> {
    if (recording) return false
    const sel = commitSelection
    let files: string[] | null = null
    let isEmptyRecord = false
    if (sel !== null) {
      await refreshStatus()
      const fresh = parseStatus(statusOutput)
      const freshSet = new Set(fresh.map((f) => f.path))
      // 目录路径不提交（后端 stage 目录 = 目录下全部文件）：勾选 UI 的目录残留会全提
      files = sel.filter((f) => !f.endsWith('/') && freshSet.has(f))
      isEmptyRecord = files.length === 0
      if (files.length > 0) {
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
    } else {
      await refreshStatus()
      const fresh = parseStatus(statusOutput)
      // 默认“全选”不自动包含已删除文件：删除必须显式勾选才会提交
      files = fresh
        .filter((f) => !f.path.endsWith('/') && f.x !== 'D')
        .map((f) => f.path)
      isEmptyRecord = files.length === 0
    }
    // 预览标记：仅标记的可预览文件生成预览（空数组 = 不生成；后端按提交文件交集过滤）
    const previewFiles = [...previewMarked].filter((f) => isPreviewable(f))

    // 空记录 / 纯里程碑：没有实际文件变更，先弹确认（避免误触生成空提交）
    if (isEmptyRecord && !rollbackState.rollback_point) {
      pendingConfirm = {
        kind: 'empty_record',
        title: '确认空记录？',
        message:
          '当前没有选择任何文件变更，将创建一条空记录 / 纯里程碑记录。\n' +
          '是否继续？',
        data: { msg, tag, tagCategory, tags, noPreview, files, previewFiles },
      }
      return false
    }

    // 回退状态下任何提交都会顶替时间线，先弹警告确认
    if (rollbackState.rollback_point) {
      pendingConfirm = {
        kind: 'empty_record',
        title: '确认回退状态记录？',
        message:
          '当前处于回退状态，提交将基于过去的版本创建新记录，并顶替当前时间线结构。\n' +
          '这可能会影响时间线，请确认是否继续？',
        data: { msg, tag, tagCategory, tags, noPreview, files, previewFiles },
      }
      return false
    }

    return performRecord(msg, tag, tagCategory, tags, noPreview, files, previewFiles)
  }

  async function performRecord(
    msg: string,
    tag: string | null,
    tagCategory: string | null,
    tags: string[],
    noPreview: boolean,
    files: string[] | null,
    previewFiles: string[],
  ): Promise<boolean> {
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
      const hash = await api.record(activePath, msg, tag, tagCategory, tags, noPreview, files, previewFiles)
      pushToast(`已记录 ${hash}`, 'success')
      previewMarked = new Set()
      if (sel !== null && files) {
        const committed = new Set(files)
        commitSelection = sel.filter((f) => !committed.has(f))
      }
      // 立即结束“记录中”状态并关闭弹窗，再后台刷新项目数据；
      // 否则大项目 full walk（loadTrackedFiles）会一直显示“记录中”/看起来卡住
      statusText = '已记录'
      setTimeout(() => { void loadProject(activePath) }, 0)
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

  function confirmRollbackRecord() {
    const p = pendingConfirm
    if (!p || p.kind !== 'empty_record') return
    const { msg, tag, tagCategory, tags, noPreview, files, previewFiles } = p.data
    pendingConfirm = null
    showCommit = false
    // 回退状态提交 = 顶替操作，显示等待遮罩，避免用户以为卡住
    busyText = '回退中记录...'
    void performRecord(msg, tag, tagCategory, tags, noPreview, files, previewFiles).finally(() => {
      busyText = ''
    })
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
      // 命令完成即清遮罩 + toast；loadProject 后台刷新（大项目 full walk/status 可能较慢，
      // 不应让“回退中”遮罩一直等待）。回退后默认全部取消选择（用户手动勾选要提交的）
      commitSelection = []
      busyText = ''
      rollbackBusy = false
      pushToast(`已回退到记录 ${entry.id.slice(0, 7)} 之前（文件保留该记录的版本，可修改后记录）`, 'success')
      setTimeout(() => { void loadProject(activePath) }, 0)
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
      // 命令完成即清遮罩 + toast；loadProject 后台刷新
      commitSelection = []
      busyText = ''
      rollbackBusy = false
      pushToast(`已丢弃回退期间的更改并回退到记录 ${entry.id.slice(0, 7)} 之前`, 'success')
      setTimeout(() => { void loadProject(activePath) }, 0)
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
      // 命令完成即清遮罩 + toast；loadProject 后台刷新（恢复后保持默认全选）。
      // 之前这里 await loadProject，若 restore 后的 status/full walk 较慢会导致 GUI 一直“恢复中”
      busyText = ''
      rollbackBusy = false
      pushToast('已恢复回退前的状态', 'success')
      setTimeout(() => { void loadProject(activePath) }, 0)
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

  // 取消更改（git restore 语义）：D（已删除）直接恢复；M（已修改）弹确认框后覆盖回 HEAD
  async function handleDiscardChanges(file: string, state: 'M' | 'D') {
    if (state === 'M') {
      pendingDiscard = { file }
      return
    }
    await doDiscard(file)
  }

  async function doDiscard(file: string) {
    console.log('[discard] doDiscard', { activePath, file })
    if (!activePath) {
      pushToast('取消更改失败: 未打开项目', 'error')
      return
    }
    try {
      await api.discardFile(activePath, file)
      pushToast(`已取消更改 ${file}`, 'success')
      // 轻量刷新：status/变更面板 + 回退状态（trackedFiles 全量 walk 不受影响）
      await Promise.all([refreshStatus(), refreshRollbackState()])
    } catch (e: any) {
      pushToast(`取消更改失败: ${e}`, 'error')
    }
  }

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
      // 命令完成即关闭冲突弹窗/清 busy；loadProject 后台刷新，避免大项目刷新让弹窗一直“恢复中”
      conflictFiles = null
      resolvingConflict = false
      pushToast('冲突已解决，已恢复回退前的状态', 'success')
      setTimeout(() => { void loadProject(activePath).then(() => refreshRollbackState()) }, 0)
      return
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
    emptyDirs = []
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
    const refreshProjectPoll = () => {
      if (activePath && projectType === 'initialized') {
        pollStatus()
        loadTrackedFiles(activePath)
        refreshRollbackState()
      }
    }
    // 不再因 document.hidden 跳过轮询：用户在资源管理器删除文件时窗口往往在后台，
    // 若暂停轮询，变更区会长时间不更新。用 pollStatus 去重防叠加即可。
    const timer = setInterval(refreshProjectPoll, 10000)
    const refreshWhenVisible = () => {
      if (!document.hidden) refreshProjectPoll()
    }
    window.addEventListener('focus', refreshWhenVisible)
    document.addEventListener('visibilitychange', refreshWhenVisible)
    return () => {
      clearInterval(timer)
      window.removeEventListener('focus', refreshWhenVisible)
      document.removeEventListener('visibilitychange', refreshWhenVisible)
      closeAskUnlisten.then((f) => f())
      blenderDlUnlisten.then((f) => f())
    }
  })
</script>

<svelte:window oncontextmenu={globalContextMenu} onkeydown={handleGlobalKeydown} />

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
      {emptyDirs}
      {activePath}
      collapsed={sidebarCollapsed}
      onExpand={() => sidebarCollapsed = false}
      onCloseProject={handleRemoveRecent}
    />

    <div class="main-area">
      {#if rollbackState.rollback_point}
        <div class="rollback-banner">
          <span class="rb-label">
            <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2.3 14.4 13.3H1.6L8 2.3z"/><path d="M8 6.6v2.8"/><circle cx="8" cy="11.6" r="0.75" fill="currentColor" stroke="none"/></svg>
            当前处于回退状态，提交将顶替时间线结构
          </span>
          <button onclick={handleRestore} disabled={rollbackBusy}>恢复</button>
        </div>
      {/if}

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
            <StatusPanel {statusOutput} {statusError} {statusErrorMsg} {trackedFiles} {rollbackChangeFiles} {rollbackDeletedFiles} {rollbackModifiedFiles} onRefresh={refreshStatus} {activePath} selection={commitSelection} rollbackActive={!!rollbackState.rollback_point} onSelectionChange={(sel) => commitSelection = sel} onDiscardChanges={handleDiscardChanges} {previewMarked} onTogglePreviewMark={togglePreviewMark} sidebarCollapsed={sidebarCollapsed} onToggleSidebar={() => sidebarCollapsed = !sidebarCollapsed} displayPath={activePath ? cleanPath(activePath) : ''} onOpenFolder={(p) => { try { openFolder(p) } catch { /* ignore */ } }} />
          {/if}
        </div>

        {#if projectType === 'initialized'}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="tl-resize" onpointerdown={startTimelineDrag} onclick={(e) => e.stopPropagation()}></div>
          <div class="timeline-panel" style="width: {timelineWidth}px">
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
              onPreviewChange={() => loadProject(activePath, true)}
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
  onRecord={openCommit}
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
  <CommitDialog onConfirm={handleRecord} onCancel={() => showCommit = false} lastTag={entries[0]?.tag || ''} lastTags={entries[0]?.tags || []} lastTagCategory={entries[0]?.tag_category || ''} busy={recording} progress={commitProgressMsg} {hasPreviewMarked} {tagGroups} />
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
  {:else if pendingConfirm.kind === 'empty_record'}
    <ConfirmDialog
      title={pendingConfirm.title}
      message={pendingConfirm.message}
      okLabel="确认记录"
      cancelLabel="取消"
      danger
      onConfirm={confirmRollbackRecord}
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

{#if pendingDiscard}
  <!-- 不用 {@const}（事件闭包作用域隐患）：onConfirm 内直接可选链取值 + 守卫 -->
  <ConfirmDialog
    title="取消更改"
    message={`将丢弃「${pendingDiscard.file}」的未提交修改，恢复到最近记录版本。\n此操作不可撤销。`}
    okLabel="取消更改"
    cancelLabel="留下"
    danger
    onConfirm={() => { const f = pendingDiscard?.file ?? ''; pendingDiscard = null; if (f) doDiscard(f) }}
    onCancel={() => (pendingDiscard = null)}
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
  .rollback-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 5px 12px;
    background: color-mix(in srgb, var(--warning) 10%, var(--surface));
    border: 1px solid color-mix(in srgb, var(--warning) 32%, transparent);
    border-radius: var(--radius);
    color: var(--text);
    font-size: 12px;
    flex-shrink: 0;
  }
  .rb-label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--warning);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rollback-banner button {
    flex-shrink: 0;
    padding: 2px 10px;
    font-size: 11px;
    font-weight: 600;
    border-radius: var(--radius-sm);
    border: 1px solid color-mix(in srgb, var(--warning) 55%, transparent);
    background: transparent;
    color: var(--warning);
    cursor: pointer;
    transition: background 0.12s var(--ease), border-color 0.12s var(--ease);
  }
  .rollback-banner button:hover {
    background: var(--warning-soft);
    border-color: var(--warning);
  }
  .content-body {
    display: flex;
    flex: 1;
    overflow: hidden;
    min-height: 0;
  }
  /* 浮动软面板：中央变更区 */
  .content-area {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--panel-radius);
    box-shadow: var(--shadow-panel);
  }
  .timeline-panel {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--panel-radius);
    box-shadow: var(--shadow-panel);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    transition: none;
  }
  .tl-resize {
    width: 9px;
    cursor: col-resize;
    flex-shrink: 0;
    background: transparent;
    border-radius: 5px;
    transition: background 0.15s var(--ease);
    z-index: 1;
  }
  .tl-resize:hover {
    background: var(--accent-soft);
  }
</style>