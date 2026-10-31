<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import * as api from '../api'

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

  let serverIp = $state('')
  let loreServerIp = $state('')
  // 两个端口走一个 IP（勾选时 loreserver IP 灰显，共用部署 IP）
  let sameIp = $state(true)
  let activeTab = $state<'main' | 'tags' | 'server'>('main')
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

  async function loadTagGroups() {
    if (!path) return
    tagLoading = true
    tagError = ''
    try {
      tagGroups = await api.getTagGroups(path)
      syncTagDrafts()
      tagsDirty = false
    } catch (e: any) {
      tagError = `读取流程与标签失败：${detailOf(e)}`
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
    const baseLabel = '新流程'
    const used = new Set(tagGroups.map((g) => g.label.trim()))
    if (!used.has(baseLabel)) return baseLabel
    let n = 2
    while (used.has(`${baseLabel} ${n}`)) n++
    return `${baseLabel} ${n}`
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
      tagError = `恢复默认失败：${detailOf(e)}`
    }
  }

  /** 校验并返回可写入的配置；错误信息直接给保存失败提示用 */
  function collectTagGroups(): api.TagGroup[] {
    const cleaned = tagGroups.map((g) => ({
      id: g.id.trim(),
      label: g.label.trim(),
      tags: g.tags.map((t) => t.trim()).filter(Boolean),
    }))
    if (cleaned.length === 0) throw new Error('至少保留一个流程')
    const seenIds = new Set<string>()
    const seenLabels = new Set<string>()
    for (const g of cleaned) {
      if (!g.id) throw new Error('流程 ID 不能为空')
      if (!g.label) throw new Error('流程名称不能为空')
      if (seenIds.has(g.id)) throw new Error(`流程 ID 重复：${g.id}`)
      if (seenLabels.has(g.label)) throw new Error(`流程名称重复：${g.label}`)
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
    dlMessage = '正在下载...'
    dlPercent = 0
    try {
      const exePath = await api.downloadBlender(selectedVersion || null)
      dlPhase = 'done'
      dlMessage = '下载完成 — 使用自带 Blender，无插件干扰 ✓'
      loadInstalled()
    } catch (e) {
      dlPhase = 'error'
      dlMessage = `下载失败: ${e}`
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
      dlMessage = `已删除 Blender ${selectedVersion}`
      await loadInstalled()
    } catch (e) {
      dlPhase = 'error'
      dlMessage = `删除失败: ${e}`
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

  async function save() {
    saving = true
    saveError = ''
    let switched = false
    // 记录当前步骤：失败时告诉用户「哪一项没存进去」，而不是静默停在"保存"
    let step = '保存部署令牌'
    try {
      // 选中已安装版本且与当前使用版本不同 → 保存时切换当前 Blender 版本
      if (selectedVersion && isInstalled(selectedVersion) && selectedVersion !== currentBlenderVersion) {
        step = '切换 Blender 版本'
        await api.setCurrentBlenderVersion(selectedVersion)
        currentBlenderVersion = selectedVersion
        switched = true
      }
      step = '保存部署令牌'
      await api.setDeployToken(deployToken)
      step = '保存部署服务器地址'
      await api.setDeployServerUrl(deployUrl())
      step = '保存备份服务器地址'
      await api.setLoreServerUrl(loreUrl())
      step = '保存自动恢复设置'
      await api.setAutoRestoreProject(autoRestoreProject)
      step = '保存提交身份'
      await api.setLoreIdentity(loreIdentity)
      step = '保存自动连接设置'
      await api.setAutoConnect(autoConnect)
      step = '保存关闭行为'
      await api.setCloseAction(closeAction === 'tray')
      step = '保存服务器开关'
      await api.setServerToggles(deployEnabled, loreEnabled)
      // 项目级流程/标签放在最后：即使写失败，全局设置也已保存（错误提示会注明）
      if (path && tagsDirty) {
        step = '保存流程与标签'
        const saved = await api.saveTagGroups(path, collectTagGroups())
        tagGroups = saved
        syncTagDrafts()
        tagsDirty = false
        onTagGroupsSaved(saved)
      }
      savedFeedback = true
      switchNotice = switched ? `当前版本已切换（Blender ${selectedVersion}）` : ''
      setTimeout(() => { savedFeedback = false; switchNotice = '' }, 2500)
    } catch (e: any) {
      const detail = typeof e === 'string' ? e : (e?.message ?? String(e))
      saveError = `${step}失败：${detail}（此前已成功的项已保存）`
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
    <h3>设置</h3>

    <div class="sd-tabs">
      <button class="sd-tab" class:active={activeTab === 'main'} onclick={() => (activeTab = 'main')}>Blender / 通用</button>
      <button class="sd-tab" class:active={activeTab === 'tags'} onclick={() => (activeTab = 'tags')}>流程 / 标签</button>
      <button class="sd-tab" class:active={activeTab === 'server'} onclick={() => (activeTab = 'server')}>服务器 / 远端仓库</button>
    </div>

    {#if activeTab === 'main'}
    <div class="section">
      <div class="section-title">Blender</div>
      <div class="field">
        <span class="hint">Keinestamp 自带独立 Blender，不影响系统已安装的版本，无插件冲突风险</span>
        <div class="dl-row">
          {#if versionsLoading}
            <span class="versions-loading">检测版本列表中...</span>
          {:else}
            <select class="sd-select" bind:value={selectedVersion} disabled={downloading}>
              {#if versions.length === 0}
                <option value="" disabled>未检测 — 点击「检测新版本」</option>
              {/if}
              {#each versions as v}
                <option value={v}>
                  {v}{v === versions[0] ? ' (最新)' : ''}{isInstalled(v) ? ' ✓' : ''}
                </option>
              {/each}
            </select>
          {/if}
          <button onclick={checkVersions} disabled={versionsLoading || cooldownLeft > 0}>
            {versionsLoading ? '检测中...' : cooldownLeft > 0 ? `冷却 ${cooldownLeft}s` : '检测新版本'}
          </button>
          {#if canDelete}
            <button class="del" class:confirm={confirmDelete} onclick={armDelete} disabled={deleting || downloading} title="至少保留一个 Blender 版本">
              {deleting ? '删除中...' : confirmDelete ? '确认删除？' : '删除'}
            </button>
          {:else}
            <button class="accent" onclick={downloadBlender} disabled={downloading || !selectedVersion || isInstalled(selectedVersion)}>
              {downloading ? '下载中...' : isInstalled(selectedVersion) ? '已安装' : '下载 Blender'}
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

    {#if activeTab === 'tags'}
    <div class="section">
      <div class="section-title">流程与标签</div>
      {#if !path}
        <div class="tag-empty">先打开一个项目后再配置流程与标签（配置保存在项目内，仅对本项目生效）</div>
      {:else if tagLoading}
        <div class="tag-empty">正在读取流程配置...</div>
      {:else if tagError}
        <div class="tag-empty error">{tagError}</div>
        <div class="flow-actions">
          <button class="mini-btn" onclick={() => void loadTagGroups()}>重试</button>
        </div>
      {:else}
        <span class="hint">流程用于给记录归类，每个流程可维护常用标签；仅对当前项目生效，默认只有一个 Blender 流程。</span>
        {#each tagGroups as group (group.id)}
          <div class="flow-card">
            <div class="flow-head">
              <input class="flow-label" type="text" bind:value={group.label} oninput={() => (tagsDirty = true)} placeholder="流程名称" aria-label="流程名称" />
              <button class="mini-btn del" onclick={() => removeFlow(group.id)} disabled={tagGroups.length <= 1} title="至少保留一个流程">删除</button>
            </div>
            <div class="flow-tags">
              {#each group.tags as tag (tag)}
                <span class="flow-tag">{tag}<button class="tag-x" onclick={() => removeTag(group, tag)} title="移除标签" aria-label="移除标签">×</button></span>
              {/each}
              {#if group.tags.length === 0}
                <span class="flow-tag-empty">暂无标签（记录时仍可输入自定义标签）</span>
              {/if}
            </div>
            <div class="flow-add">
              <input
                type="text"
                bind:value={tagDrafts[group.id]}
                placeholder="输入标签后回车添加"
                aria-label="新标签"
                onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); addTag(group) } }}
              />
              <button class="mini-btn" onclick={() => addTag(group)} disabled={!(tagDrafts[group.id] || '').trim()}>添加</button>
            </div>
          </div>
        {/each}
        <div class="flow-actions">
          <button class="mini-btn" onclick={addFlow}>+ 添加流程</button>
          <button class="mini-btn" onclick={() => void resetTagGroups()} title="恢复为默认的单个 Blender 流程（点击保存后才写入）">恢复默认</button>
          {#if tagsDirty}
            <span class="flow-dirty">有未保存的修改</span>
          {/if}
        </div>
      {/if}
    </div>
    {/if}

    {#if activeTab === 'server'}
    <div class="section">
      <div class="section-title">服务器</div>
      <div class="svc-toggle-row">
        <button class="svc-toggle" class:on={deployEnabled} onclick={() => (deployEnabled = !deployEnabled)} title="开启/关闭网页部署服务">
          <span class="svc-dot"></span>网页部署
        </button>
        <button class="svc-toggle" class:on={loreEnabled} onclick={() => (loreEnabled = !loreEnabled)} title="开启/关闭 loreserver 服务">
          <span class="svc-dot"></span>loreserver
        </button>
      </div>
      <div class="conn-grid">
        <div class="conn-row">
          <span class="port-head">网页部署</span>
          <div class="input-row">
            <span class="port-label">IP</span>
            <input class="port-input ip-input" type="text" bind:value={serverIp} placeholder="192.168.0.100" aria-label="网页部署服务器 IP" />
            <span class="port-label">端口</span>
            <input class="port-input port-num" type="text" bind:value={deployPort} placeholder="7799" aria-label="部署端口" />
          </div>
        </div>
        <div class="conn-row">
          <span class="port-head">loreserver</span>
          <div class="input-row">
            <span class="port-label">IP</span>
            <input class="port-input ip-input" type="text" bind:value={loreServerIp} disabled={sameIp} placeholder="192.168.0.100" aria-label="loreserver IP" class:sd-disabled={sameIp} />
            <span class="port-label">端口</span>
            <input class="port-input port-num" type="text" bind:value={lorePort} placeholder="41337" aria-label="loreserver 端口" />
          </div>
        </div>
      </div>
      <div class="field toggle-field">
        <label>
          <input type="checkbox" bind:checked={sameIp} />
          统一 IP（loreserver 共用网页部署 IP）
        </label>
      </div>
      <div class="field">
        <div class="input-row test-row">
          <button class="connect-btn" onclick={testConnection} disabled={testing || (!deployEnabled && !loreEnabled)}>
            {testing ? '测试中...' : '测试连接'}
          </button>
          {#if connectStatus !== 'none' || loreStatus !== 'none'}
            <span class="conn-result">
              网页部署 {#if connectStatus === 'ok'}{@render iconOk()}{:else if connectStatus === 'fail'}{@render iconFail()}{:else}—{/if}
              · lore {#if loreStatus === 'ok'}{@render iconOk()}{:else if loreStatus === 'fail'}{@render iconFail()}{:else}—{/if}
            </span>
          {/if}
        </div>
      </div>
      <div class="field">
        <label for="sd-deploy-token">部署令牌（可选）</label>
        <input id="sd-deploy-token" type="password" bind:value={deployToken} placeholder="Bearer Token" />
        <span class="hint">上传认证用；loreserver 无认证，仅限可信局域网</span>
      </div>
      <div class="field toggle-field">
        <label>
          <input type="checkbox" bind:checked={autoConnect} />
          打开项目时自动测试连接
        </label>
      </div>
      <div class="section-title">远端仓库</div>
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

    {#if activeTab === 'main'}
    <div class="section">
      <div class="section-title">通用</div>
      <div class="field toggle-field">
        <label>
          <input type="checkbox" bind:checked={autoRestoreProject} />
          启动时自动打开上次的项目
        </label>
      </div>
      <div class="field">
        <div class="label-row">
          <label for="sd-lore-identity">提交身份</label>
          <button type="button" class="tip" aria-label="提交身份的命名规则">
            ?
            <span class="tip-bubble" role="tooltip">
              <b>命名自由，不要求 @local</b>
              <span>推荐「名字@主机」形式：para@local / zhangsan@studio-pc</span>
              <span>留空则回落到默认 keinestamp@local</span>
              <span>只影响之后的新记录，历史作者不变</span>
            </span>
          </button>
        </div>
        <input id="sd-lore-identity" type="text" bind:value={loreIdentity} placeholder="keinestamp@local" />
        <span class="hint">记录提交的作者标识（程序级全局设置）</span>
      </div>
      <div class="field">
        <label for="sd-close-action">关闭窗口时</label>
        <select id="sd-close-action" class="sd-select" bind:value={closeAction}>
          <option value="tray">后台驻留（通知区域）</option>
          <option value="quit">退出应用</option>
        </select>
        <span class="hint">首次关闭会询问一次，之后按此习惯执行；通知区域图标左键打开、右键可随时退出</span>
      </div>
    </div>
    {/if}

    <div class="actions">
      {#if saveError}
        <span class="save-error" role="alert" title={saveError}>{saveError}</span>
      {/if}
      <button onclick={onClose}>取消</button>
      <button class="accent" onclick={save} disabled={saving} title={switchNotice || saveError || ''}>{saving ? '保存中…' : switchNotice ? '已切换 ✓' : savedFeedback ? '已保存 ✓' : '保存'}</button>
    </div>
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
  .actions { display: flex; gap: 6px; justify-content: flex-end; margin-top: 16px; }
</style>