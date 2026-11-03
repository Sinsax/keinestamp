<script lang="ts">
  import { onMount } from 'svelte'
  import { t, localeEpoch } from './lib/i18n/index.svelte'
  import { errorCode, stripErrorCode } from './lib/errors'
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
  let rollbackState = $state<RollbackState>({ rollback_point: null, head_matches: false, deleted_files: [], change_files: [], modified_files: [], restore_modified_files: [] })
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

  // 语言切换后：状态面板 / 时间线 / 日志里的文案由后端生成，需要按新语言重新拉一次
  let localeSeen = -1
  $effect(() => {
    const epoch = localeEpoch()
    if (localeSeen === -1) { localeSeen = epoch; return }
    if (epoch === localeSeen) return
    localeSeen = epoch
    void refreshStatus().catch(() => {})
    if (activePath) void loadProject(activePath, true).catch(() => {})
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
    // 错误消息可能带 [code:xxx] 前缀（控制流用），展示前剥掉
    msg = stripErrorCode(msg)
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
    if (!deployStatus) return t('gui.actionbar.deployTip')
    const r = deployStatus.remote ?? '—'
    return deployAhead
      ? t('gui.app.deployAhead', { local: deployStatus.local, remote: r })
      : t('gui.app.deployUpToDate', { local: deployStatus.local, remote: r })
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
    rollbackState = { rollback_point: null, head_matches: false, deleted_files: [], change_files: [], modified_files: [], restore_modified_files: [] }
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
        pushToast(t('gui.app.err.configMissing'), 'error')
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
    busyText = t('gui.app.busy.reinit')
    try {
      await api.initProject(path)
      pushToast(t('gui.app.toast.configRegenerated'), 'success')
      await loadRecent()
      await loadProject(path)
    } catch (e: any) {
      pushToast(t('gui.app.err.reinitFailed', { error: e }), 'error')
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
        title: t('gui.app.confirm.emptyRecordTitle'),
        message:
          t('gui.app.confirm.emptyRecordLine1') +
          t('gui.app.confirm.emptyRecordLine2'),
        data: { msg, tag, tagCategory, tags, noPreview, files, previewFiles },
      }
      return false
    }

    // 回退状态下任何提交都会顶替时间线，先弹警告确认
    if (rollbackState.rollback_point) {
      // 回退态下「一个都没勾选」不等于空提交：后端会把它补成全部回退检出文件。
      // 文案必须说清提交范围，否则用户以为什么都没提交（2026-10 审查 P1）
      const checkedOut = rollbackState.change_files?.length ?? 0
      const scopeNote =
        files.length === 0 && checkedOut > 0
          ? t('gui.app.confirm.rollbackScopeAll', { n: checkedOut }) + '\n'
          : files.length > 0
            ? t('gui.app.confirm.rollbackScopeFiles', { n: files.length }) + '\n'
            : ''
      pendingConfirm = {
        kind: 'empty_record',
        title: t('gui.app.confirm.rollbackRecordTitle'),
        message:
          scopeNote +
          t('gui.app.confirm.rollbackRecordLine1') +
          t('gui.app.confirm.rollbackRecordLine2'),
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
      statusText = t('gui.app.status.recording')
      unlisten = await listen<CommitProgress>('commit-progress', (event) => {
        statusText = event.payload.message
        commitProgressMsg = event.payload.message
      })
      const sel = commitSelection
      const hash = await api.record(activePath, msg, tag, tagCategory, tags, noPreview, files, previewFiles)
      pushToast(t('gui.app.toast.recorded', { hash }), 'success')
      previewMarked = new Set()
      if (sel !== null && files) {
        const committed = new Set(files)
        commitSelection = sel.filter((f) => !committed.has(f))
      }
      // 立即结束“记录中”状态并关闭弹窗，再后台刷新项目数据；
      // 否则大项目 full walk（loadTrackedFiles）会一直显示“记录中”/看起来卡住
      statusText = t('gui.app.status.recorded')
      setTimeout(() => { void loadProject(activePath) }, 0)
      return true
    } catch (e: any) {
      pushToast(t('gui.app.err.recordFailed', { error: e }), 'error')
      statusText = t('gui.app.status.recordFailed')
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
    busyText = t('gui.app.busy.rollbackRecord')
    void performRecord(msg, tag, tagCategory, tags, noPreview, files, previewFiles).finally(() => {
      busyText = ''
    })
  }

  async function handleDeploy() {
    if (!activePath) return
    deploying = true
    statusText = t('gui.app.status.deploying')
    try {
      await api.deploy(activePath)
      pushToast(t('gui.app.toast.deployed'), 'success')
      await testConnection()
      await loadProject(activePath)
      statusText = t('gui.app.status.deployed')
    } catch (e: any) {
      pushToast(t('gui.app.err.deployFailed', { error: e }), 'error')
      statusText = t('gui.app.status.deployFailed')
    } finally {
      deploying = false
    }
  }

  async function handleBackup() {
    if (!activePath) return
    backupBusy = true
    statusText = t('gui.app.status.backingUp')
    try {
      await api.backup(activePath)
      pushToast(t('gui.app.toast.backedUp'), 'success')
      statusText = t('gui.app.status.backedUp')
    } catch (e: any) {
      const msg = String(e)
      if (errorCode(msg) === 'offline_repo') {
        // 离线仓库无法备份 → 引导迁移为在线仓库
        pendingMigrate = activePath
      } else {
        pushToast(t('gui.app.err.backupFailed', { error: msg }), 'error')
      }
      statusText = t('gui.app.status.backupFailed')
    } finally {
      backupBusy = false
    }
  }

  async function confirmMigrateAndBackup() {
    const path = pendingMigrate
    pendingMigrate = null
    if (!path) return
    backupBusy = true
    busyText = t('gui.app.busy.migrate')
    try {
      await api.migrateRepo(path)
      pushToast(t('gui.app.toast.migrated'), 'success')
      await api.backup(path)
      pushToast(t('gui.app.toast.backedUp'), 'success')
      await loadProject(path)
    } catch (e: any) {
      pushToast(t('gui.app.err.migrateBackupFailed', { error: e }), 'error')
    }
    busyText = ''
    backupBusy = false
  }

  function handleTokenChange(val: string) {
    accessToken = val
    pushToast(t('gui.app.toast.tokenSaved'), 'success')
  }

  async function handleSiteNameChange(val: string): Promise<boolean> {
    if (!/^[A-Za-z0-9_-]+$/.test(val)) {
      pushToast(t('gui.app.toast.siteNameInvalid'), 'error')
      return false
    }
    try {
      await api.updateConfig(activePath, 'site_name', val || null)
      siteName = val
      pushToast(t('gui.app.toast.siteNameSaved'), 'success')
      return true
    } catch (e: any) {
      pushToast(t('gui.app.err.siteNameSaveFailed', { error: e }), 'error')
      return false
    }
  }

  async function handleOpenSite() {
    if (!siteName) {
      pushToast(t('gui.app.toast.siteNameMissing'), 'info')
      return
    }
    try {
      const serverUrl = await api.getDeployServerUrl()
      if (!serverUrl) {
        pushToast(t('gui.app.toast.serverMissing'), 'info')
        return
      }
      const url = `${serverUrl.replace(/\/+$/, '')}/${siteName}/`
      await api.openSite(url)
    } catch (e: any) {
      pushToast(t('gui.app.err.openSiteFailed', { error: e }), 'error')
    }
  }

  async function handleLocalView() {
    if (!activePath) return
    try {
      await api.openWebTimeline(activePath)
    } catch (e: any) {
      pushToast(t('gui.app.err.localViewFailed', { error: e }), 'error')
    }
  }

  async function refreshRollbackState() {
    const path = activePath
    if (!path || projectType !== 'initialized') return
    try {
      const st = await api.getRollbackState(path)
      // 切项目后旧项目的响应不能覆盖新项目状态（否则横幅/恢复按钮会在错误项目上出现）
      if (path !== activePath) return
      rollbackState = st
    } catch (e: any) {
      if (path !== activePath) return
      console.error('[rollbackState] error:', e)
    }
  }

  async function handleRollback(entry: TimelineEntry) {    if (!activePath || rollbackBusy) return
    rollbackBusy = true
    busyText = t('gui.app.busy.rollback')
    try {
      await api.rollback(activePath, entry.id)
      // 命令完成即清遮罩 + toast；loadProject 后台刷新（大项目 full walk/status 可能较慢，
      // 不应让“回退中”遮罩一直等待）。回退后默认全部取消选择（用户手动勾选要提交的）
      commitSelection = []
      busyText = ''
      rollbackBusy = false
      pushToast(t('gui.app.toast.rolledBack', { id: entry.id.slice(0, 7) }), 'success')
      setTimeout(() => { void loadProject(activePath) }, 0)
      return
    } catch (e: any) {
      const msg = String(e)
      if (errorCode(msg) === 'rollback_discard_confirm') {
        // 连续回退：回退期间的更改内容（上次回退检出的中间态）需丢弃确认
        const files = rollbackState.modified_files?.length ? '\n' + t('gui.app.confirm.files', { files: rollbackState.modified_files.join(', ') }) : ''
        pendingConfirm = {
          kind: 'rollback',
          entry,
          title: t('gui.app.confirm.discardRollbackTitle'),
          message: t('gui.app.confirm.discardRollbackIntro') + files + '\n' + t('gui.app.confirm.discardRollbackAsk', { id: entry.id.slice(0, 7) }),
        }
      } else {
        pushToast(t('gui.app.err.rollbackFailed', { error: msg }), 'error')
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
    busyText = t('gui.app.busy.rollbackDiscard')
    try {
      await api.rollback(activePath, entry.id, true)
      // 命令完成即清遮罩 + toast；loadProject 后台刷新
      commitSelection = []
      busyText = ''
      rollbackBusy = false
      pushToast(t('gui.app.toast.rolledBackDiscarded', { id: entry.id.slice(0, 7) }), 'success')
      setTimeout(() => { void loadProject(activePath) }, 0)
      return
    } catch (e: any) {
      pushToast(t('gui.app.err.rollbackFailed', { error: e }), 'error')
    }
    busyText = ''
    rollbackBusy = false
  }

  async function handleRestore() {
    if (!activePath) return
    // 恢复确认：只有"真修改"（用户改过/删过内容）才弹窗；回退删除标记属检出，直接恢复。
    // 只看跟踪文件修改（restore_modified_files）——未跟踪新文件恢复时不会被删除，不进确认列表
    const hasChanges = (rollbackState.restore_modified_files?.length ?? 0) > 0
    if (hasChanges) {
      const files = rollbackState.restore_modified_files?.length ? '\n' + t('gui.app.confirm.files', { files: rollbackState.restore_modified_files.join(', ') }) : ''
      pendingConfirm = {
        kind: 'restore',
        title: t('gui.app.confirm.restoreTitle'),
        message: t('gui.app.confirm.restoreIntro') + files + '\n' + t('gui.app.confirm.restoreLine1') + '\n' + t('gui.app.confirm.restoreLine2'),
      }
      return
    }
    await doRestore(null)
  }

  async function doRestore(confirm: boolean | null) {
    if (!activePath) return
    rollbackBusy = true
    busyText = t('gui.app.busy.restore')
    try {
      await api.restoreRollback(activePath, confirm)
      // 命令完成即清遮罩 + toast；loadProject 后台刷新（恢复后保持默认全选）。
      // 之前这里 await loadProject，若 restore 后的 status/full walk 较慢会导致 GUI 一直“恢复中”
      busyText = ''
      rollbackBusy = false
      pushToast(t('gui.app.toast.restored'), 'success')
      setTimeout(() => { void loadProject(activePath) }, 0)
      return
    } catch (e: any) {
      const msg = String(e)
      if (errorCode(msg) === 'rollback_conflict') {
        await loadConflictFiles()
      } else if (errorCode(msg) === 'restore_confirm') {
        // 快照过时/后端实时判定有真修改 → 弹确认窗（后端为权威判定源；只列跟踪文件修改）
        const files = rollbackState.restore_modified_files?.length ? '\n' + t('gui.app.confirm.files', { files: rollbackState.restore_modified_files.join(', ') }) : ''
        pendingConfirm = {
          kind: 'restore',
          title: t('gui.app.confirm.restoreTitle'),
          message: t('gui.app.confirm.restoreIntro') + files + '\n' + t('gui.app.confirm.restoreLine1') + '\n' + t('gui.app.confirm.restoreLine2'),
        }
      } else {
        pushToast(t('gui.app.err.restoreFailed', { error: msg }), 'error')
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
      pushToast(t('gui.app.err.discardNoProject'), 'error')
      return
    }
    try {
      await api.discardFile(activePath, file)
      pushToast(t('gui.app.toast.discarded', { file }), 'success')
      // 轻量刷新：status/变更面板 + 回退状态（trackedFiles 全量 walk 不受影响）
      await Promise.all([refreshStatus(), refreshRollbackState()])
    } catch (e: any) {
      pushToast(t('gui.app.err.discardFailed', { error: e }), 'error')
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
      pushToast(t('gui.app.err.conflictReadFailed', { error: e }), 'error')
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
      pushToast(t('gui.app.toast.conflictResolved'), 'success')
      setTimeout(() => { void loadProject(activePath).then(() => refreshRollbackState()) }, 0)
      return
    } catch (e: any) {
      pushToast(t('gui.app.err.resolveFailed', { error: e }), 'error')
    }
    resolvingConflict = false
  }

  async function handleExportEntry(entry: TimelineEntry) {
    const selected = await open({
      directory: true,
      multiple: false,
      title: t('gui.app.dialog.exportTitle'),
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
          loreError = t('gui.app.toast.loreEnvError')
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
        pushToast(t('gui.app.toast.blenderDownloading'), 'info')
      } else if (blenderDlActive && phase === 'done') {
        blenderDlActive = false
        pushToast(t('gui.app.toast.blenderDownloaded'), 'success')
      } else if (blenderDlActive && phase === 'error') {
        blenderDlActive = false
        pushToast(t('gui.app.toast.blenderDownloadFailed'), 'error')
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
            {t('gui.app.rollbackBanner')}
          </span>
          <button onclick={handleRestore} disabled={rollbackBusy}>{t('gui.actionbar.restore')}</button>
        </div>
      {/if}

      <div class="content-body">
        <div class="content-area">
          {#if !activePath}
            <div class="empty-state">
              <div class="big"><svg viewBox="0 0 24 24" width="32" height="32" fill="currentColor" opacity="0.3"><path d="M2 6a2 2 0 012-2h5.757a2 2 0 011.414.586l1.12 1.12A2 2 0 0013.757 6H20a2 2 0 012 2v8a2 2 0 01-2 2H4a2 2 0 01-2-2V6z"/></svg></div>
              {t('gui.app.empty.pickProject')}
              <button class="empty-new-btn" onclick={handleNewProject}>{t('gui.app.empty.newProject')}</button>
            </div>
          {:else if projectType !== 'initialized'}
            <div class="empty-state">
              <div class="big"><svg viewBox="0 0 24 24" width="32" height="32" fill="currentColor" opacity="0.5"><path d="M12 2a1 1 0 01.894.553l9 18A1 1 0 0121 22H3a1 1 0 01-.894-1.447l9-18A1 1 0 0112 2zm0 3.236L4.472 20h15.056L12 5.236z"/><path d="M12 9a.5.5 0 01.5.5v5a.5.5 0 01-1 0v-5A.5.5 0 0112 9z"/><circle cx="12" cy="16.5" r=".5"/></svg></div>
              <div>{t('gui.app.empty.notInitialized')}</div>
              {t('gui.app.empty.reopenFolder')}
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
              {tagGroups}
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
          ? t('gui.app.restore.none')
          : rollbackState.head_matches
            ? t('gui.app.restore.ready')
            : t('gui.app.restore.blocked')}
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
      okLabel={t('gui.app.dialog.discardAndJump')}
      cancelLabel={t('gui.app.dialog.keep')}
      onConfirm={confirmDiscardRollback}
      onCancel={() => (pendingConfirm = null)}
    />
  {:else if pendingConfirm.kind === 'empty_record'}
    <ConfirmDialog
      title={pendingConfirm.title}
      message={pendingConfirm.message}
      okLabel={t('gui.app.dialog.confirmRecord')}
      cancelLabel={t('common.cancel')}
      danger
      onConfirm={confirmRollbackRecord}
      onCancel={() => (pendingConfirm = null)}
    />
  {:else}
    <ConfirmDialog
      title={pendingConfirm.title}
      message={pendingConfirm.message}
      okLabel={t('gui.app.dialog.discardAndRestore')}
      cancelLabel={t('gui.app.dialog.keep')}
      onConfirm={() => { pendingConfirm = null; doRestore(true) }}
      onCancel={() => (pendingConfirm = null)}
    />
  {/if}
{/if}

{#if pendingMigrate}
  <ConfirmDialog
    title={t('gui.app.dialog.migrateTitle')}
    message={t('gui.app.dialog.migrateMessage')}
    okLabel={t('gui.app.dialog.migrateOk')}
    cancelLabel={t('common.cancel')}
    onConfirm={confirmMigrateAndBackup}
    onCancel={() => (pendingMigrate = null)}
  />
{/if}

{#if pendingDiscard}
  <!-- 不用 {@const}（事件闭包作用域隐患）：onConfirm 内直接可选链取值 + 守卫 -->
  <ConfirmDialog
    title={t('gui.app.dialog.discardTitle')}
    message={t('gui.app.dialog.discardMessage', { file: pendingDiscard.file })}
    okLabel={t('gui.app.dialog.discardOk')}
    cancelLabel={t('gui.app.dialog.keep')}
    danger
    onConfirm={() => { const f = pendingDiscard?.file ?? ''; pendingDiscard = null; if (f) doDiscard(f) }}
    onCancel={() => (pendingDiscard = null)}
  />
{/if}

{#if showLoreMissing}
  <ConfirmDialog
    title={t('gui.app.dialog.loreMissingTitle')}
    message={t('gui.app.dialog.loreMissingMessage', { error: loreError || t('gui.app.dialog.loreMissingFallback') })}
    okLabel={t('gui.app.dialog.loreMissingOk')}
    cancelLabel=""
    onConfirm={() => (showLoreMissing = false)}
    onCancel={() => (showLoreMissing = false)}
  />
{/if}

{#if pendingReinit}
  <ConfirmDialog
    title={t('gui.app.dialog.reinitTitle')}
    message={t('gui.app.dialog.reinitMessage', { name: pendingReinit.split(/[/\\]/).pop() ?? '' })}
    okLabel={t('gui.app.dialog.reinitOk')}
    cancelLabel={t('common.cancel')}
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
    title={t('gui.app.dialog.removeTitle')}
    message={t('gui.app.dialog.removeMessage', { name: pendingRemove.split(/[/\\]/).pop() ?? '' })}
    okLabel={t('gui.app.dialog.removeOk')}
    cancelLabel={t('common.cancel')}
    onConfirm={confirmRemoveRecent}
    onCancel={() => pendingRemove = null}
  />
{/if}

{#if pendingExport}
  <InputDialog
    title={t('gui.app.dialog.exportTitle')}
    label={t('gui.app.dialog.exportLabel')}
    initial={pendingExport.defaultName}
    okLabel={t('gui.app.dialog.exportOk')}
    cancelLabel={t('common.cancel')}
    onConfirm={confirmExport}
    onCancel={() => pendingExport = null}
  />
{/if}

{#if showSettings}
  <SettingsDialog
    path={activePath || ''}
    recentPaths={recentProjects.map((r) => r.path)}
    onOpenProject={(p) => { showSettings = false; selectRecent(p) }}
    onTagGroupsSaved={(groups) => (tagGroups = groups)}
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
