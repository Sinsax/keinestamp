<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import * as api from '../api'
  import type { AboutInfo, UpdateInfo } from '../types'

  let {
    path = '',
    recentPaths = [],
    onOpenProject = (_path: string) => {},
    onTagGroupsSaved = (_groups: api.TagGroup[]) => {},
    onClose,
  }: {
    path?: string
    recentPaths?: string[]
    onOpenProject?: (path: string) => void
    /** 保存流程/标签后回传规范化结果，主界面立即刷新（记录弹窗与时间线筛选） */
    onTagGroupsSaved?: (groups: api.TagGroup[]) => void
    onClose: () => void
  } = $props()

  import Remote from './Remote.svelte'
  import { t, setLocale, syncSystemLocale, getLocale, getSystemLocale, nativeName, type Locale, type LocalePref } from './i18n/index.svelte'

  let serverIp = $state('')
  let loreServerIp = $state('')
  // 两个端口走一个 IP（勾选时 loreserver IP 灰显，共用部署 IP）
  let sameIp = $state(true)
  let activeTab = $state<'main' | 'tags' | 'server' | 'about'>('main')
  let savedLoreUrl = $state('')
  // 服务开关（网页部署 / loreserver；false=关闭，测试与远端列表按此启用）
  let deployEnabled = $state(false)
  let loreEnabled = $state(false)
  let deployPort = $state('7799')
  let lorePort = $state('41337')
  let deployToken = $state('')
  let autoRestoreProject = $state(false)
  let loreIdentity = $state('keinestamp@local')
  let autoConnect = $state(false)
  let closeAction = $state<'tray' | 'quit'>('tray')
  // 语言：偏好落 app-config.toml，切换即时生效（不等「保存」），托盘菜单同步重建
  let localePref = $state<LocalePref>('system')
  let localeSaving = $state(false)
  let selectedVersion = $state('')
  let versions = $state<string[]>([])
  let installedVersions = $state<string[]>([])
  let currentBlenderVersion = $state('')
  let switchNotice = $state('')
  let versionsLoading = $state(false)
  let cooldownLeft = $state(0)
  let cooldownUntil = 0
  let cooldownTimer: ReturnType<typeof setTimeout> | undefined
  let saving = $state(false)
  let downloading = $state(false)
  let deleting = $state(false)
  let confirmDelete = $state(false)
  let confirmDeleteTimer: ReturnType<typeof setTimeout> | undefined
  let dlPhase = $state('')
  let dlMessage = $state('')
  let dlPercent = $state(0)
  let testing = $state(false)
  let connectStatus = $state<'none' | 'ok' | 'fail'>('none')
  let loreStatus = $state<'none' | 'ok' | 'fail'>('none')
  let savedFeedback = $state(false)
  // 保存失败必须可见：整块 catch{} 会让"保存失败"和"保存中"长得一模一样，
  // 而且前面已成功的项已经落盘（半保存状态），用户完全无法察觉（2026-10 审查 P1）
  let saveError = $state('')
  // 流程 / 标签（项目级 .keinestamp/tags.toml；与其他设置一起在「保存」时写入）
  let tagGroups = $state<api.TagGroup[]>([])
  let tagDrafts = $state<Record<string, string>>({})
  let tagLoading = $state(false)
  let tagError = $state('')
  let tagsDirty = $state(false)
  // 标签输入框：默认折叠成「+ 添加标签」，点开后才出现输入框（减少每张卡片的常驻噪音）
  let editingTagFor = $state<string | null>(null)
  let tagInputEl = $state<HTMLInputElement | null>(null)
  // 删除流程：先点一次进入确认态，4 秒内再点才真删（避免误触）
  let confirmFlowId = $state<string | null>(null)
  let confirmFlowTimer: ReturnType<typeof setTimeout> | undefined
  // 关于页：版本 / Lore 运行时 / 路径 / 更新检查（进入该页才读取，避免每次开设置都 spawn lore）
  let about = $state<AboutInfo | null>(null)
  let aboutLoading = $state(false)
  let aboutError = $state('')
  let pathError = $state('')
  let update = $state<UpdateInfo | null>(null)
  let updateLoading = $state(false)
  let updateError = $state('')

  onMount(() => {
    (async () => {
      try {
        deployToken = await api.getDeployToken()
        const [deployUrl, loreUrl] = await Promise.all([api.getDeployServerUrl(), api.getLoreServerUrl()])
        savedLoreUrl = loreUrl || ''
        // 从 URL 反向解析 IP/端口（免协议前缀输入）
        const host = (u: string, fb: string): string => u.match(/^[a-z]+:\/\/([^:/]+)/)?.[1] || fb
        const port = (u: string, fb: string): string => u.match(/:(\d+)(?:\/|$)/)?.[1] || fb
        const deployHost = host(deployUrl || '', '')
        const loreHost = host(loreUrl || '', '')
        serverIp = deployHost || loreHost
        loreServerIp = loreHost
        // 两 IP 相同或未配置 loreserver IP → 勾选"两个端口走一个 IP"
        sameIp = !loreHost || loreHost === serverIp
        deployPort = port(deployUrl || '', '7799')
        lorePort = port(loreUrl || '', '41337')
        const localeInfo = await api.getEffectiveLocale()
        localePref = (localeInfo.pref || 'system') as LocalePref
        syncSystemLocale(localeInfo.system_tag as Locale)
        autoRestoreProject = await api.getAutoRestoreProject()
        loreIdentity = await api.getLoreIdentity()
        autoConnect = await api.getAutoConnect()
        const [deployOn, loreOn] = await api.getServerToggles()
        deployEnabled = deployOn
        loreEnabled = loreOn
        const close = await api.getCloseAction()
        closeAction = close === false ? 'quit' : 'tray'
      } catch { /* ignore */ }
    })()

    ;(async () => {
      try {
        const cached = await api.getCachedBlenderVersions()
        if (cached.length > 0) {
          versions = cached
          if (!selectedVersion) selectedVersion = versions[0]
        }
      } catch { /* ignore */ }
    })()

    loadInstalled()
    if (path) void loadTagGroups()

    const unlistenPromise = listen<BlenderProgress>('blender-download-progress', (event) => {
      dlPhase = event.payload.phase
      dlMessage = event.payload.message
      dlPercent = event.payload.percent
    })
    return () => { unlistenPromise.then(fn => fn()) }
  })

  onDestroy(() => {
    if (cooldownTimer) clearTimeout(cooldownTimer)
    if (confirmFlowTimer) clearTimeout(confirmFlowTimer)
  })

  function updateCooldown() {
    const left = Math.max(0, Math.ceil((cooldownUntil - Date.now()) / 1000))
    cooldownLeft = left
    if (left > 0) {
      cooldownTimer = setTimeout(updateCooldown, 500)
    }
  }

  async function checkVersions() {
    if (versionsLoading || cooldownLeft > 0) return
    versionsLoading = true
    try {
      versions = await api.refreshBlenderVersions()
      if (versions.length > 0 && !selectedVersion) {
        selectedVersion = versions[0]
      }
    } catch { /* ignore */ }
    versionsLoading = false
    cooldownUntil = Date.now() + 10000
    updateCooldown()
  }

  async function loadInstalled() {
    try {
      installedVersions = await api.listInstalledBlenderVersions()
    } catch { /* ignore */ }
    try {
      currentBlenderVersion = await api.getCurrentBlenderVersion()
    } catch { /* ignore */ }
  }

  function detailOf(e: any): string {
    return typeof e === 'string' ? e : (e?.message ?? String(e))
  }

    /** 语言切换即时生效：写偏好 → 后端同步进程内 locale + 重建托盘 → 前端重渲染 */
  async function applyLocale(event: Event) {
    if (localeSaving) return
    // 以 select 当前值为准（不依赖 bind 与 change 的先后顺序）
    const select = event.currentTarget as HTMLSelectElement | null
    if (select) localePref = select.value as LocalePref
    localeSaving = true
    try {
      const info = await api.setLocalePref(localePref)
      localePref = (info.pref || 'system') as LocalePref
      syncSystemLocale(info.system_tag as Locale)
      setLocale(info.tag as Locale)
    } catch (e) {
      // 失败就退回后端当前状态，避免下拉与实际语言不一致
      const info = await api.getEffectiveLocale().catch(() => null)
      if (info) {
        localePref = (info.pref || 'system') as LocalePref
        setLocale(info.tag as Locale)
      }
      saveError = String(e)
    } finally {
      localeSaving = false
    }
  }

async function loadTagGroups() {
    if (!path) return
    tagLoading = true
    tagError = ''
    try {
      tagGroups = await api.getTagGroups(path)
      syncTagDrafts()
      tagsDirty = false
    } catch (e: any) {
      tagError = t('gui.settings.err.loadTags', { error: detailOf(e) })
    } finally {
      tagLoading = false
    }
  }

  function syncTagDrafts() {
    const next: Record<string, string> = {}
    for (const g of tagGroups) next[g.id] = tagDrafts[g.id] ?? ''
    tagDrafts = next
  }

  // 流程 id 写入历史记录的 tag_category；删除流程后不复用旧 id，避免旧记录被错误归属
  function nextFlowId(): string {
    const used = new Set(tagGroups.map((g) => g.id))
    let id = ''
    do {
      id = `flow-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`
    } while (used.has(id))
    return id
  }

  function nextFlowLabel(): string {
    const baseLabel = t('gui.settings.tags.newFlowLabel')
    const used = new Set(tagGroups.map((g) => g.label.trim()))
    if (!used.has(baseLabel)) return baseLabel
    let n = 2
    while (used.has(`${baseLabel} ${n}`)) n++
    return `${baseLabel} ${n}`
  }

  /** 展开某个流程的标签输入框并聚焦 */
  async function startAddTag(groupId: string) {
    editingTagFor = groupId
    await tick()
    tagInputEl?.focus()
  }

  function cancelAddTag() {
    editingTagFor = null
  }

  /** 删除流程需要二次确认：第一次点击进入确认态，4 秒无操作自动还原 */
  function armRemoveFlow(id: string) {
    if (tagGroups.length <= 1) return
    if (confirmFlowId === id) {
      confirmFlowId = null
      if (confirmFlowTimer) clearTimeout(confirmFlowTimer)
      removeFlow(id)
      return
    }
    confirmFlowId = id
    if (confirmFlowTimer) clearTimeout(confirmFlowTimer)
    confirmFlowTimer = setTimeout(() => {
      confirmFlowId = null
    }, 4000)
  }

  function addFlow() {
    const id = nextFlowId()
    tagGroups = [...tagGroups, { id, label: nextFlowLabel(), tags: [] }]
    tagDrafts[id] = ''
    tagsDirty = true
  }

  function removeFlow(id: string) {
    if (tagGroups.length <= 1) return
    tagGroups = tagGroups.filter((g) => g.id !== id)
    const next = { ...tagDrafts }
    delete next[id]
    tagDrafts = next
    tagsDirty = true
  }

  function addTag(group: api.TagGroup) {
    const tag = (tagDrafts[group.id] || '').trim()
    if (!tag) return
    if (!group.tags.includes(tag)) group.tags = [...group.tags, tag]
    tagDrafts[group.id] = ''
    tagsDirty = true
  }

  function removeTag(group: api.TagGroup, tag: string) {
    group.tags = group.tags.filter((t) => t !== tag)
    tagsDirty = true
  }

  async function resetTagGroups() {
    try {
      const defaults = await api.getDefaultTagGroups()
      tagGroups = defaults.map((g) => ({ ...g, tags: [...g.tags] }))
      syncTagDrafts()
      tagsDirty = true
    } catch (e: any) {
      tagError = t('gui.settings.err.resetTags', { error: detailOf(e) })
    }
  }

  /** 校验并返回可写入的配置；错误信息直接给保存失败提示用 */
  function collectTagGroups(): api.TagGroup[] {
    const cleaned = tagGroups.map((g) => ({
      id: g.id.trim(),
      label: g.label.trim(),
      tags: g.tags.map((t) => t.trim()).filter(Boolean),
    }))
    if (cleaned.length === 0) throw new Error(t('gui.settings.err.keepOneFlow'))
    const seenIds = new Set<string>()
    const seenLabels = new Set<string>()
    for (const g of cleaned) {
      if (!g.id) throw new Error(t('gui.settings.err.flowIdEmpty'))
      if (!g.label) throw new Error(t('gui.settings.err.flowLabelEmpty'))
      if (seenIds.has(g.id)) throw new Error(t('gui.settings.err.flowIdDup', { id: g.id }))
      if (seenLabels.has(g.label)) throw new Error(t('gui.settings.err.flowLabelDup', { label: g.label }))
      seenIds.add(g.id)
      seenLabels.add(g.label)
    }
    return cleaned
  }

  function isInstalled(v: string): boolean {
    return installedVersions.includes(v)
  }

  async function downloadBlender() {
    downloading = true
    dlPhase = 'downloading'
    dlMessage = t('gui.settings.blender.dlDownloading')
    dlPercent = 0
    try {
      const exePath = await api.downloadBlender(selectedVersion || null)
      dlPhase = 'done'
      dlMessage = t('gui.settings.blender.dlDone')
      loadInstalled()
    } catch (e) {
      dlPhase = 'error'
      dlMessage = t('gui.settings.err.dlFailed', { error: String(e) })
      console.error('blender download failed', e)
    }
    downloading = false
  }

  // 删除 Blender 版本：二次点击确认（唯一版本后端拒绝 + 前端不显示删除按钮）
  function armDelete() {
    if (confirmDelete) {
      performDelete()
      return
    }
    confirmDelete = true
    if (confirmDeleteTimer) clearTimeout(confirmDeleteTimer)
    confirmDeleteTimer = setTimeout(() => { confirmDelete = false }, 3000)
  }

  async function performDelete() {
    if (!selectedVersion) return
    deleting = true
    confirmDelete = false
    if (confirmDeleteTimer) clearTimeout(confirmDeleteTimer)
    try {
      await api.deleteBlenderVersion(selectedVersion)
      dlPhase = 'done'
      dlMessage = t('gui.settings.blender.dlDeleted', { version: selectedVersion })
      await loadInstalled()
    } catch (e) {
      dlPhase = 'error'
      dlMessage = t('gui.settings.err.deleteFailed', { error: String(e) })
    }
    deleting = false
  }

  const canDelete = $derived(installedVersions.length > 1 && !!selectedVersion && isInstalled(selectedVersion))

  function deployUrl(): string {
    const ip = serverIp.trim()
    return ip ? `http://${ip}:${deployPort.trim() || '7799'}` : ''
  }

  function loreUrl(): string {
    const ip = (sameIp ? serverIp : loreServerIp).trim()
    return ip ? `lore://${ip}:${lorePort.trim() || '41337'}` : ''
  }

  async function testConnection() {
    const url = deployUrl()
    const ip = serverIp.trim()
    testing = true
    connectStatus = 'none'
    loreStatus = 'none'
    // 部署服务器 + loreserver 双状态探测（按服务开关：关闭的服务不测试 → 状态 —）
    if (deployEnabled && url) {
      try {
        const ok = await api.testConnection(url, deployToken || null)
        connectStatus = ok ? 'ok' : 'fail'
      } catch {
        connectStatus = 'fail'
      }
    } else {
      connectStatus = 'none'
    }
    if (loreEnabled && ip) {
      try {
        const loreHost = (sameIp ? serverIp : loreServerIp).trim()
        loreStatus = (await api.testLoreConnection(loreHost)) ? 'ok' : 'fail'
      } catch {
        loreStatus = 'fail'
      }
    } else {
      loreStatus = 'none'
    }
    testing = false
  }

  // ---- 关于页 ----

  /** 切到关于页：首次进入才读数据 / 检查更新（按钮上不预先发网络请求） */
  function openAbout() {
    activeTab = 'about'
    if (!about && !aboutLoading) void loadAbout()
    if (!update && !updateLoading) void loadUpdate()
  }

  async function loadAbout() {
    aboutLoading = true
    aboutError = ''
    pathError = ''
    try {
      about = await api.getAboutInfo()
    } catch (e: any) {
      aboutError = detailOf(e)
    } finally {
      aboutLoading = false
    }
  }

  async function loadUpdate() {
    if (updateLoading) return
    updateLoading = true
    updateError = ''
    try {
      update = await api.checkAppUpdate()
    } catch (e: any) {
      updateError = detailOf(e)
    } finally {
      updateLoading = false
    }
  }

  function checkedAtText(rfc: string): string {
    const d = new Date(rfc)
    return Number.isNaN(d.getTime()) ? rfc : d.toLocaleString()
  }

  async function openPath(target: string) {
    if (!target) return
    pathError = ''
    try {
      await api.openFolder(target)
    } catch (e: any) {
      pathError = detailOf(e)
    }
  }

  async function openLink(url: string) {
    if (!url) return
    pathError = ''
    try {
      await api.openSite(url)
    } catch (e: any) {
      pathError = detailOf(e)
    }
  }

  async function save() {
    saving = true
    saveError = ''
    let switched = false
    // 记录当前步骤：失败时告诉用户「哪一项没存进去」，而不是静默停在"保存"
    let step = t('gui.settings.steps.deployToken')
    try {
      // 选中已安装版本且与当前使用版本不同 → 保存时切换当前 Blender 版本
      if (selectedVersion && isInstalled(selectedVersion) && selectedVersion !== currentBlenderVersion) {
        step = t('gui.settings.steps.blenderVersion')
        await api.setCurrentBlenderVersion(selectedVersion)
        currentBlenderVersion = selectedVersion
        switched = true
      }
      step = t('gui.settings.steps.deployToken')
      await api.setDeployToken(deployToken)
      step = t('gui.settings.steps.deployUrl')
      await api.setDeployServerUrl(deployUrl())
      step = t('gui.settings.steps.loreUrl')
      await api.setLoreServerUrl(loreUrl())
      step = t('gui.settings.steps.autoRestore')
      await api.setAutoRestoreProject(autoRestoreProject)
      step = t('gui.settings.steps.identity')
      await api.setLoreIdentity(loreIdentity)
      step = t('gui.settings.steps.autoConnect')
      await api.setAutoConnect(autoConnect)
      step = t('gui.settings.steps.closeBehavior')
      await api.setCloseAction(closeAction === 'tray')
      step = t('gui.settings.steps.serverToggles')
      await api.setServerToggles(deployEnabled, loreEnabled)
      // 项目级流程/标签放在最后：即使写失败，全局设置也已保存（错误提示会注明）
      if (path && tagsDirty) {
        step = t('gui.settings.steps.tags')
        const saved = await api.saveTagGroups(path, collectTagGroups())
        tagGroups = saved
        syncTagDrafts()
        tagsDirty = false
        onTagGroupsSaved(saved)
      }
      savedFeedback = true
      switchNotice = switched ? t('gui.settings.steps.switchedNotice', { version: selectedVersion }) : ''
      setTimeout(() => { savedFeedback = false; switchNotice = '' }, 2500)
    } catch (e: any) {
      const detail = typeof e === 'string' ? e : (e?.message ?? String(e))
      saveError = t('gui.settings.steps.saveFailed', { step, error: detail })
      console.error('[settings] save failed at', step, e)
    } finally {
      saving = false
    }
  }

  interface BlenderProgress {
    phase: string
    message: string
    percent: number
    bytes_downloaded: number
    bytes_total: number
  }
</script>

{#snippet iconOk()}
  <svg class="st-icon ok" viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="8" cy="8" r="6.4" /><path d="M5.3 8.2l1.9 1.9 3.5-3.9" /></svg>
{/snippet}
{#snippet iconFail()}
  <svg class="st-icon fail" viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="8" cy="8" r="6.4" /><path d="M5.6 5.6l4.8 4.8M10.4 5.6l-4.8 4.8" /></svg>
{/snippet}

<div class="dialog-overlay" onclick={onClose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog settings-panel" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>{t('common.settings')}</h3>

    <div class="sd-tabs">
      <button class="sd-tab" class:active={activeTab === 'main'} onclick={() => (activeTab = 'main')}>{t('gui.settings.tabs.general')}</button>
      <button class="sd-tab" class:active={activeTab === 'tags'} onclick={() => (activeTab = 'tags')}>{t('gui.settings.tabs.tags')}</button>
      <button class="sd-tab" class:active={activeTab === 'server'} onclick={() => (activeTab = 'server')}>{t('gui.settings.tabs.server')}</button>
      <button class="sd-tab" class:active={activeTab === 'about'} onclick={openAbout}>
        {t('gui.settings.tabs.about')}
        {#if update?.state === 'available'}<span class="sd-dot" title={t('gui.settings.updateAvailable')} aria-label={t('gui.settings.updateAvailable')}></span>{/if}
      </button>
    </div>

    {#if activeTab === 'main'}
    <div class="section">
      <div class="section-title">{t('gui.settings.general.sectionTitle')}</div>
      <div class="field toggle-field">
        <label>
          <input type="checkbox" bind:checked={autoRestoreProject} />
          {t('gui.settings.general.autoRestore')}
        </label>
      </div>
      <div class="field">
        <label for="sd-language">{t('gui.settings.language')}</label>
        <select
          id="sd-language"
          class="sd-select"
          bind:value={localePref}
          disabled={localeSaving}
          onchange={applyLocale}
        >
          <option value="system">
            {t('gui.settings.languageSystemCurrent', { lang: nativeName(localePref === 'system' ? getSystemLocale() : getLocale()) })}
          </option>
          <option value="zh-CN">{t('gui.settings.languageZhCn')}</option>
          <option value="en-US">{t('gui.settings.languageEnUs')}</option>
        </select>
        <span class="hint">{t('gui.settings.languageHint')}</span>
      </div>
      <div class="field">
        <div class="label-row">
          <label for="sd-lore-identity">{t('gui.settings.general.identityLabel')}</label>
          <button type="button" class="tip" aria-label={t('gui.settings.general.identityAria')}>
            ?
            <span class="tip-bubble" role="tooltip">
              <b>{t('gui.settings.general.identityFree')}</b>
              <span>{t('gui.settings.general.identityTip1')}</span>
              <span>{t('gui.settings.general.identityTip2')}</span>
              <span>{t('gui.settings.general.identityTip3')}</span>
            </span>
          </button>
        </div>
        <input id="sd-lore-identity" type="text" bind:value={loreIdentity} placeholder="keinestamp@local" />
        <span class="hint">{t('gui.settings.general.identityHint')}</span>
      </div>
      <div class="field">
        <label for="sd-close-action">{t('gui.settings.general.closeLabel')}</label>
        <select id="sd-close-action" class="sd-select" bind:value={closeAction}>
          <option value="tray">{t('gui.settings.general.closeTray')}</option>
          <option value="quit">{t('gui.settings.general.closeQuit')}</option>
        </select>
        <span class="hint">{t('gui.settings.general.closeHint')}</span>
      </div>
    </div>
    {/if}

    {#if activeTab === 'tags'}
    <div class="section">
      {#if !path}
        <div class="tag-empty">{t('gui.settings.tags.noProject')}</div>
      {:else if tagLoading}
        <div class="tag-empty">{t('gui.settings.tags.loading')}</div>
      {:else if tagError}
        <div class="tag-empty error">{tagError}</div>
        <div class="flow-actions">
          <button class="mini-btn" onclick={() => void loadTagGroups()}>{t('gui.settings.tags.retry')}</button>
        </div>
      {:else}
        <div class="tags-head">
          <div class="section-title">{t('gui.settings.tags.sectionTitle')}</div>
          <button type="button" class="tip" aria-label={t('gui.settings.tags.helpAria')}>
            ?
            <span class="tip-bubble" role="tooltip">
              <b>{t('gui.settings.tags.helpTitle')}</b>
              <span>{t('gui.settings.tags.helpWhat')}</span>
              <span>{t('gui.settings.tags.helpRecord')}</span>
              <span>{t('gui.settings.tags.helpScope')}</span>
              <span>{t('gui.settings.tags.helpSave')}</span>
            </span>
          </button>
          {#if tagsDirty}
            <span class="flow-dirty">{t('gui.settings.tags.dirty')}</span>
          {/if}
        </div>
        <span class="hint">{t('gui.settings.tags.hint')}</span>
        {#each tagGroups as group, i (group.id)}
          <div class="flow-card">
            <div class="flow-head">
              <span class="flow-index" aria-hidden="true">{i + 1}</span>
              <input
                class="flow-label"
                type="text"
                bind:value={group.label}
                oninput={() => (tagsDirty = true)}
                placeholder={t('gui.settings.tags.flowNamePlaceholder')}
                aria-label={t('gui.settings.tags.flowNameAria')}
                title={t('gui.settings.tags.flowNameTip')}
              />
              <span class="flow-count">{t('gui.settings.tags.tagCount', { n: group.tags.length })}</span>
              <button
                class="mini-btn del"
                class:armed={confirmFlowId === group.id}
                onclick={() => armRemoveFlow(group.id)}
                disabled={tagGroups.length <= 1}
                title={tagGroups.length <= 1 ? t('gui.settings.err.keepOneFlow') : t('gui.settings.tags.deleteFlowTipReasons')}
              >
                {confirmFlowId === group.id ? t('gui.settings.tags.deleteFlowArmed') : t('gui.settings.tags.deleteFlow')}
              </button>
            </div>
            <div class="flow-tags">
              {#each group.tags as tag (tag)}
                <span class="flow-tag" title={t('gui.settings.tags.tagChipTip')}>
                  {tag}
                  <button class="tag-x" onclick={() => removeTag(group, tag)} title={t('gui.settings.tags.removeTag')} aria-label={t('gui.settings.tags.removeTag')}>×</button>
                </span>
              {/each}
              {#if group.tags.length === 0}
                <span class="flow-tag-empty">{t('gui.settings.tags.emptyTags')}</span>
              {/if}
            </div>
            {#if editingTagFor === group.id}
              <div class="flow-add">
                <input
                  type="text"
                  bind:this={tagInputEl}
                  bind:value={tagDrafts[group.id]}
                  placeholder={t('gui.settings.tags.tagPlaceholder')}
                  aria-label={t('gui.settings.tags.newTagAria')}
                  onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addTag(group) } else if (e.key === 'Escape') { e.preventDefault(); cancelAddTag() } }}
                  onblur={() => { if (!(tagDrafts[group.id] || '').trim()) editingTagFor = null }}
                />
                <button class="mini-btn" onclick={() => addTag(group)} disabled={!(tagDrafts[group.id] || '').trim()}>{t('common.add')}</button>
                <button class="mini-btn ghost" onclick={cancelAddTag}>{t('common.cancel')}</button>
              </div>
            {:else}
              <button class="flow-add-btn" onclick={() => startAddTag(group.id)} title={t('gui.settings.tags.addTagTip')}>
                + {t('gui.settings.tags.addTag')}
              </button>
            {/if}
          </div>
        {/each}
        <div class="flow-actions">
          <button class="mini-btn" onclick={addFlow} title={t('gui.settings.tags.addFlowTip')}>+ {t('gui.settings.tags.addFlow')}</button>
          <button class="mini-btn ghost" onclick={() => void resetTagGroups()} title={t('gui.settings.tags.resetTip2')}>{t('gui.settings.tags.reset')}</button>
        </div>
      {/if}
    </div>
    {/if}

    {#if activeTab === 'server'}
    <div class="section">
      <div class="section-title">{t('gui.settings.server.sectionTitle')}</div>
      <div class="svc-toggle-row">
        <button class="svc-toggle" class:on={deployEnabled} onclick={() => (deployEnabled = !deployEnabled)} title={t('gui.settings.server.deployToggleTitle')}>
          <span class="svc-dot"></span>{t('gui.settings.server.deploy')}
        </button>
        <button class="svc-toggle" class:on={loreEnabled} onclick={() => (loreEnabled = !loreEnabled)} title={t('gui.settings.server.loreToggleTitle')}>
          <span class="svc-dot"></span>loreserver
        </button>
      </div>
      <div class="conn-grid">
        <div class="conn-row">
          <span class="port-head">{t('gui.settings.server.deploy')}</span>
          <div class="input-row">
            <span class="port-label">IP</span>
            <input class="port-input ip-input" type="text" bind:value={serverIp} placeholder="192.168.0.100" aria-label={t('gui.settings.server.ipAria')} />
            <span class="port-label">{t('gui.settings.server.port')}</span>
            <input class="port-input port-num" type="text" bind:value={deployPort} placeholder="7799" aria-label={t('gui.settings.server.deployPortAria')} />
          </div>
        </div>
        <div class="conn-row">
          <span class="port-head">loreserver</span>
          <div class="input-row">
            <span class="port-label">IP</span>
            <input class="port-input ip-input" type="text" bind:value={loreServerIp} disabled={sameIp} placeholder="192.168.0.100" aria-label="loreserver IP" class:sd-disabled={sameIp} />
            <span class="port-label">{t('gui.settings.server.port')}</span>
            <input class="port-input port-num" type="text" bind:value={lorePort} placeholder="41337" aria-label={t('gui.settings.server.lorePortAria')} />
          </div>
        </div>
      </div>
      <div class="field toggle-field">
        <label>
          <input type="checkbox" bind:checked={sameIp} />
          {t('gui.settings.server.sameIp')}
        </label>
      </div>
      <div class="field">
        <div class="input-row test-row">
          <button class="connect-btn" onclick={testConnection} disabled={testing || (!deployEnabled && !loreEnabled)}>
            {testing ? t('gui.settings.server.testing') : t('gui.settings.server.test')}
          </button>
          {#if connectStatus !== 'none' || loreStatus !== 'none'}
            <span class="conn-result">
              {t('gui.settings.server.deploy')} {#if connectStatus === 'ok'}{@render iconOk()}{:else if connectStatus === 'fail'}{@render iconFail()}{:else}—{/if}
              · lore {#if loreStatus === 'ok'}{@render iconOk()}{:else if loreStatus === 'fail'}{@render iconFail()}{:else}—{/if}
            </span>
          {/if}
        </div>
      </div>
      <div class="field">
        <label for="sd-deploy-token">{t('gui.settings.server.tokenLabel')}</label>
        <input id="sd-deploy-token" type="password" bind:value={deployToken} placeholder="Bearer Token" />
        <span class="hint">{t('gui.settings.server.tokenHint')}</span>
      </div>
      <div class="field toggle-field">
        <label>
          <input type="checkbox" bind:checked={autoConnect} />
          {t('gui.settings.server.autoConnect')}
        </label>
      </div>
      <div class="section-title">{t('gui.settings.server.remoteSection')}</div>
      <Remote
        loreServerUrl={savedLoreUrl}
        recentPaths={recentPaths}
        enabled={loreEnabled}
        embedded
        onOpenProject={onOpenProject}
        onGoSettings={() => (activeTab = 'server')}
      />
    </div>
    {/if}

    {#if activeTab === 'about'}
    <div class="section">
      <div class="section-title">{t('gui.settings.about.software')}</div>
      {#if aboutLoading && !about}
        <div class="about-empty">{t('gui.settings.about.loading')}</div>
      {:else if about}
        <div class="about-row">
          <span class="about-label">{t('gui.settings.about.version')}</span>
          <span class="about-value">
            <span>{about.name} <b>v{about.version}</b></span>
          </span>
        </div>
        <div class="about-row">
          <span class="about-label">{t('gui.settings.about.platform')}</span>
          <span class="about-value">{about.platform}</span>
        </div>
        <div class="about-row">
          <span class="about-label">Lore</span>
          <span class="about-value">
            {#if about.lore}
              <span class="about-inline ok">{@render iconOk()} v{about.lore}</span>
            {:else}
              <span class="about-inline bad">{@render iconFail()} {t('gui.settings.about.unavailable')}</span>
            {/if}
          </span>
        </div>
        {#if about.lore_error}
          <div class="about-error" role="alert">{about.lore_error}</div>
        {/if}
        <span class="hint">{t('gui.settings.about.licenseHint', { license: about.license, version: about.lore_expected })}</span>
      {:else if aboutError}
        <div class="about-empty error">{aboutError}</div>
        <div class="flow-actions">
          <button class="mini-btn" onclick={() => void loadAbout()}>{t('gui.settings.tags.retry')}</button>
        </div>
      {/if}
    </div>

    <div class="section">
      <div class="section-title">{t('gui.settings.about.location')}</div>
      {#if about}
        <div class="about-row">
          <span class="about-label">{t('gui.settings.about.exeDir')}</span>
          <span class="about-path" title={about.exe_dir}>{about.exe_dir}</span>
          <button class="mini-btn" onclick={() => void openPath(about?.exe_dir || '')}>{t('gui.settings.about.open')}</button>
        </div>
        <div class="about-row">
          <span class="about-label">{t('gui.settings.about.configDir')}</span>
          <span class="about-path" title={about.config_dir}>{about.config_dir}</span>
          <button class="mini-btn" onclick={() => void openPath(about?.config_dir || '')}>{t('gui.settings.about.open')}</button>
        </div>
      {/if}
      {#if pathError}
        <div class="about-error" role="alert">{pathError}</div>
      {/if}
    </div>

    <div class="section">
      <div class="section-title">{t('gui.settings.about.updateSection')}</div>
      <div class="about-row">
        <span class="about-label">{t('gui.settings.about.repo')}</span>
        <span class="about-path" title={about?.project_url}>{about?.project_url}</span>
        <button class="mini-btn" onclick={() => void openLink(about?.project_url || '')}>{t('gui.settings.about.open')}</button>
      </div>
      <div class="about-row">
        <span class="about-label">{t('gui.settings.about.status')}</span>
        <span class="about-value">
          {#if updateLoading}
            {t('gui.settings.about.checkingUpdate')}
          {:else if update}
            {#if update.state === 'available'}
              <button class="about-link" onclick={() => void openLink(update?.url || '')} title={t('gui.settings.about.openRelease')}>
                {t('gui.settings.about.found', { version: update.latest ?? '' })}
              </button>
              <span>{t('gui.settings.about.current', { version: update.current })}</span>
            {:else if update.state === 'up_to_date'}
              <span class="about-inline ok">{@render iconOk()} {update.message}</span>
            {:else}
              <span class="about-unknown">{update.message}</span>
            {/if}
          {:else if updateError}
            <span class="about-unknown">{t('gui.settings.about.checkFailed', { error: updateError })}</span>
          {:else}
            {t('gui.settings.about.notChecked')}
          {/if}
        </span>
      </div>
      <div class="about-actions">
        <button class="mini-btn" onclick={() => void loadUpdate()} disabled={updateLoading}>{updateLoading ? t('gui.settings.about.checkingUpdate') : t('gui.settings.about.checkNow')}</button>
      </div>
      {#if update?.checked_at}
        <span class="hint">{t('gui.settings.about.lastChecked', { time: checkedAtText(update.checked_at) })}</span>
      {/if}
    </div>
    {/if}

    {#if activeTab === 'main'}
    <div class="section">
      <div class="section-title">Blender</div>
      <div class="field">
        <span class="hint">{t('gui.settings.blender.hint')}</span>
        <div class="dl-row">
          {#if versionsLoading}
            <span class="versions-loading">{t('gui.settings.blender.detecting')}</span>
          {:else}
            <select class="sd-select" bind:value={selectedVersion} disabled={downloading}>
              {#if versions.length === 0}
                <option value="" disabled>{t('gui.settings.blender.notDetected')}</option>
              {/if}
              {#each versions as v}
                <option value={v}>
                  {v}{v === versions[0] ? t('gui.settings.blender.latestSuffix') : ''}{isInstalled(v) ? ' ✓' : ''}
                </option>
              {/each}
            </select>
          {/if}
          <button onclick={checkVersions} disabled={versionsLoading || cooldownLeft > 0}>
            {versionsLoading ? t('gui.settings.blender.checking') : cooldownLeft > 0 ? t('gui.settings.blender.cooldown', { n: cooldownLeft }) : t('gui.settings.blender.checkNew')}
          </button>
          {#if canDelete}
            <button class="del" class:confirm={confirmDelete} onclick={armDelete} disabled={deleting || downloading} title={t('gui.settings.blender.keepOne')}>
              {deleting ? t('gui.settings.blender.deleting') : confirmDelete ? t('gui.settings.blender.confirmDelete') : t('common.delete')}
            </button>
          {:else}
            <button class="accent" onclick={downloadBlender} disabled={downloading || !selectedVersion || isInstalled(selectedVersion)}>
              {downloading ? t('gui.settings.blender.downloading') : isInstalled(selectedVersion) ? t('gui.settings.blender.installed') : t('gui.settings.blender.download')}
            </button>
          {/if}
        </div>
        {#if dlPhase === 'downloading'}
          <div class="progress-bar-container">
            <div class="progress-bar" style="width: {dlPercent}%"></div>
          </div>
          <div class="progress-info">
            <span>{dlMessage}</span>
            <span class="percent">{dlPercent}%</span>
          </div>
        {:else if dlPhase === 'extracting'}
          <div class="progress-bar-container">
            <div class="progress-bar indeterminate"></div>
          </div>
          <div class="progress-info">
            <span>{dlMessage}</span>
          </div>
        {:else if dlPhase === 'done'}
          <span class="dl-status success">{dlMessage}</span>
        {:else if dlPhase === 'error'}
          <span class="dl-status error">{dlMessage}</span>
        {/if}
      </div>
    </div>
    {/if}

    {#if activeTab !== 'about'}
    <div class="actions">
      {#if saveError}
        <span class="save-error" role="alert" title={saveError}>{saveError}</span>
      {/if}
      <button onclick={onClose}>{t('common.cancel')}</button>
      <button class="accent" onclick={save} disabled={saving} title={switchNotice || saveError || ''}>{saving ? t('gui.settings.general.saveSaving') : switchNotice ? t('gui.settings.general.saveSwitched') : savedFeedback ? t('gui.settings.general.saveSaved') : t('gui.settings.general.save')}</button>
    </div>
    {/if}
  </div>
</div>

<style>
  /* 保存失败提示：贴在操作区左侧，长文本省略但 title 可完整查看 */
  .save-error {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
    color: var(--error);
    text-align: left;
  }
  .sd-tabs {
    display: flex;
    gap: 2px;
    margin-bottom: 14px;
    padding: 2px;
    background: var(--surface2);
    border-radius: var(--radius);
  }
  .sd-tab {
    flex: 1;
    padding: 4px 12px;
    border-radius: calc(var(--radius) - 2px);
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    font-size: 12px;
    transition: background 0.12s, color 0.12s;
  }
  .sd-tab:hover {
    color: var(--text);
  }
  .sd-tab.active {
    background: var(--surface);
    color: var(--text);
    font-weight: 600;
    box-shadow: var(--shadow-sm);
  }
  input.sd-disabled,
  input:disabled {
    opacity: 0.45;
    background: var(--surface2);
    cursor: not-allowed;
  }
  .settings-panel {
    width: 520px;
    max-height: 82vh;
    overflow-y: auto;
  }
  h3 { margin-bottom: 14px; font-size: 15px; font-weight: 650; }
  .section { margin-bottom: 16px; padding-bottom: 14px; border-bottom: 1px solid var(--border); }
  .section:last-of-type { border-bottom: none; margin-bottom: 0; }
  .section-title { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 1px; color: var(--text-dim); margin-bottom: 8px; }
  .field { margin-bottom: 12px; }
  .field:last-child { margin-bottom: 0; }
  .field label { display: block; font-size: 11.5px; color: var(--text-dim); margin-bottom: 4px; }
  .field input[type="text"], .field input[type="password"] { width: 100%; }
  .hint { display: block; font-size: 10px; color: var(--text-dim); margin-top: 3px; }
  .label-row { display: flex; align-items: center; gap: 6px; margin-bottom: 4px; }
  .label-row label { margin-bottom: 0; }
  /* 提交身份命名说明：悬停/聚焦 ? 显示气泡；面板可滚动，气泡宽度控制在面板内 */
  .tip {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    padding: 0;
    border-radius: 50%;
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-dim);
    font-size: 9.5px;
    font-weight: 700;
    line-height: 1;
    cursor: help;
    flex-shrink: 0;
  }
  .tip:hover { border-color: var(--tint); background: transparent; color: var(--tint); }
  .tip:focus-visible { border-color: var(--tint); color: var(--tint); outline: none; }
  .tip-bubble {
    display: none;
    position: absolute;
    left: -6px;
    top: calc(100% + 7px);
    z-index: 20;
    width: 316px;
    padding: 8px 10px;
    flex-direction: column;
    gap: 2px;
    font-size: 10.5px;
    font-weight: 400;
    line-height: 1.55;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-md);
    pointer-events: none;
  }
  .tip-bubble b { font-weight: 650; }
  .tip:hover .tip-bubble, .tip:focus-visible .tip-bubble { display: flex; }
  .dl-row { display: flex; align-items: center; gap: 8px; margin-top: 6px; }
  .dl-row button { font-size: 12px; padding: 3px 12px; }
  .dl-row .del {
    background: color-mix(in srgb, var(--error) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--error) 40%, transparent);
    color: var(--error);
  }
  .dl-row .del.confirm {
    background: var(--error);
    border-color: var(--error);
    color: #fff;
    font-weight: 600;
  }
  .dl-row .del:disabled { opacity: 0.5; }
  .dl-row select, .sd-select { width: 200px; font-size: 12px; padding: 3px 8px; background: var(--surface); color: var(--text); border: 1px solid var(--border-strong); border-radius: var(--radius-sm); }
  .versions-loading { font-size: 11px; color: var(--text-dim); }

  .progress-bar-container {
    width: 100%; height: 4px;
    background: var(--surface3); border-radius: 2px;
    margin-top: 10px; overflow: hidden;
    position: relative;
  }
  .progress-bar {
    height: 100%; background: var(--accent);
    border-radius: 2px;
    transition: width 0.3s ease;
  }
  .progress-bar.indeterminate {
    width: 30% !important;
    animation: indeterminate 1.5s infinite ease-in-out;
  }
  @keyframes indeterminate {
    0% { margin-left: 0; }
    50% { margin-left: 70%; }
    100% { margin-left: 0; }
  }
  .progress-info {
    display: flex; justify-content: space-between;
    font-size: 10.5px; color: var(--text-dim);
    margin-top: 4px;
  }
  .percent { font-weight: 600; }

  .dl-status { font-size: 10.5px; color: var(--text-dim); margin-top: 4px; display: block; }
  .dl-status.success { color: var(--success); }
  .dl-status.error { color: var(--error); }
  .toggle-field label { display: flex; align-items: center; gap: 8px; font-size: 12.5px; cursor: pointer; color: var(--text); }
  .toggle-field input[type="checkbox"] { width: auto; }
.input-row { display: flex; align-items: center; gap: 8px; }
.input-row input { flex: 1; }
.port-label { font-size: 10.5px; color: var(--text-dim); white-space: nowrap; }
.port-input { flex: 0 0 70px !important; width: 70px; }
.ip-input { flex: 1 1 auto !important; width: auto; }
.port-num { flex: 0 0 60px !important; width: 60px; }
.conn-grid { display: flex; flex-direction: column; gap: 7px; margin-bottom: 10px; }
.conn-row { display: flex; align-items: center; gap: 10px; }
.conn-row .port-head { width: 74px; flex-shrink: 0; margin: 0; font-size: 11.5px; color: var(--text-dim); }
.conn-row .input-row { flex: 1; }
.test-row { margin-top: 2px; }
.test-row .connect-btn { flex-shrink: 0; }
.svc-toggle-row { display: flex; gap: 6px; margin-bottom: 10px; }
.svc-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-strong);
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
  font-size: 12px;
  transition: all 0.12s;
}
.svc-toggle:hover { border-color: var(--tint); color: var(--text); }
.svc-toggle.on {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
  font-weight: 600;
}
.svc-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--border-strong);
  flex-shrink: 0;
  transition: background 0.12s;
}
.svc-toggle.on .svc-dot { background: #fff; }
.st-icon { flex-shrink: 0; vertical-align: -2px; }
.st-icon.ok { color: var(--success); }
.st-icon.fail { color: var(--error); }
.connect-btn { font-size: 11px; padding: 3px 12px; border-radius: var(--radius-sm); cursor: pointer; background: var(--surface); border: 1px solid var(--border-strong); color: var(--text); white-space: nowrap; }
  .connect-btn:hover:not(:disabled) { background: var(--surface2); }
  .connect-btn:disabled { opacity: 0.4; cursor: default; }
.conn-result { display: block; font-size: 11px; margin-top: 4px; color: var(--text-dim); }
  /* 流程 / 标签设置 */
  .tag-empty { font-size: 11.5px; color: var(--text-dim); padding: 4px 0; }
  .tag-empty.error { color: var(--error); }
  .flow-card {
    margin-top: 10px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface2);
  }
  .flow-head { display: flex; align-items: center; gap: 8px; }
  .flow-label { flex: 1; min-width: 0; font-size: 12px; padding: 3px 8px; }
  .mini-btn {
    font-size: 11px;
    padding: 3px 10px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    color: var(--text);
    white-space: nowrap;
  }
  .mini-btn:hover:not(:disabled) { background: var(--surface3); }
  .mini-btn:disabled { opacity: 0.4; cursor: default; }
  .mini-btn.del { color: var(--error); border-color: color-mix(in srgb, var(--error) 35%, transparent); }
  .flow-tags { display: flex; flex-wrap: wrap; align-items: center; gap: 5px; margin-top: 7px; }
  .flow-tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    padding: 1px 4px 1px 8px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--text);
  }
  .tag-x {
    border: none;
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    font-size: 12px;
    line-height: 1;
    padding: 0 3px;
    border-radius: 50%;
  }
  .tag-x:hover { color: var(--error); background: transparent; }
  .flow-tag-empty { font-size: 10.5px; color: var(--text-dim); }
  .flow-add { display: flex; gap: 6px; margin-top: 7px; }
  .flow-add input { flex: 1; min-width: 0; font-size: 11.5px; padding: 2px 8px; }
  .flow-actions { display: flex; align-items: center; gap: 6px; margin-top: 10px; }
  .flow-dirty { font-size: 10.5px; color: var(--accent); margin-left: auto; }
  /* 流程 / 标签：标题行（标题 + 说明气泡 + 未保存标记） */
  .tags-head { display: flex; align-items: center; gap: 6px; margin-bottom: 4px; }
  .tags-head .section-title { margin: 0; }
  .tags-head .tip-bubble { left: auto; right: -8px; }
  .flow-index {
    font-size: 10px; color: var(--text-dim); min-width: 12px; text-align: right;
    font-variant-numeric: tabular-nums; flex-shrink: 0;
  }
  .flow-count { font-size: 10px; color: var(--text-dim); white-space: nowrap; flex-shrink: 0; }
  /* 折叠态：虚线胶囊按钮，比常驻输入框安静 */
  .flow-add-btn {
    margin-top: 8px; font-size: 11px; padding: 2px 10px;
    border: 1px dashed var(--border-strong); border-radius: 999px;
    background: transparent; color: var(--text-dim); cursor: pointer;
    transition: color 0.12s var(--ease), border-color 0.12s var(--ease);
  }
  .flow-add-btn:hover { color: var(--accent); border-color: var(--accent); }
  .mini-btn.ghost { background: transparent; border-color: transparent; color: var(--text-dim); }
  .mini-btn.ghost:hover:not(:disabled) { background: var(--surface3); color: var(--text); }
  .mini-btn.del.armed {
    background: color-mix(in srgb, var(--error) 18%, transparent);
    border-color: var(--error); color: var(--error); font-weight: 600;
  }
  /* 卡片稍提亮，和面板底色区分开 */
  /* 关于页 */
  .sd-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-left: 4px;
    border-radius: 50%;
    background: var(--accent);
    vertical-align: 2px;
  }
  .about-row { display: flex; align-items: center; gap: 8px; padding: 2px 0; min-height: 20px; }
  .about-label { flex: 0 0 56px; font-size: 11px; color: var(--text-dim); }
  .about-value { flex: 1; min-width: 0; display: flex; flex-wrap: wrap; align-items: center; gap: 6px; font-size: 11.5px; }
  .about-path {
    flex: 1;
    min-width: 0;
    font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
    font-size: 10.5px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .about-path:hover { color: var(--text); }
  .about-inline { display: inline-flex; align-items: center; gap: 4px; }
  .about-inline.ok { color: var(--success); }
  .about-inline.bad { color: var(--error); }
  .about-link {
    font-size: 11.5px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--accent);
    cursor: pointer;
    white-space: nowrap;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .about-link:hover { color: var(--text); }
  .about-unknown { color: var(--text-dim); }
  .about-empty { font-size: 11.5px; color: var(--text-dim); padding: 4px 0; }
  .about-empty.error { color: var(--error); }
  .about-error { font-size: 10.5px; color: var(--error); margin-top: 4px; word-break: break-word; }
  .about-actions { display: flex; align-items: center; gap: 6px; margin-top: 10px; }
  .actions { display: flex; gap: 6px; justify-content: flex-end; margin-top: 16px; }
</style>
