<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import * as api from '../api'

  let {
    path = '',
    onClose,
  }: {
    path?: string
    onClose: () => void
  } = $props()

  let serverIp = $state('')
  let deployPort = $state('7799')
  let lorePort = $state('41337')
  let deployToken = $state('')
  let autoRestoreProject = $state(false)
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

  onMount(() => {
    (async () => {
      try {
        deployToken = await api.getDeployToken()
        const [deployUrl, loreUrl] = await Promise.all([api.getDeployServerUrl(), api.getLoreServerUrl()])
        // 从 URL 反向解析 IP/端口（免协议前缀输入）
        const host = (u: string, fb: string): string => u.match(/^[a-z]+:\/\/([^:/]+)/)?.[1] || fb
        const port = (u: string, fb: string): string => u.match(/:(\d+)(?:\/|$)/)?.[1] || fb
        serverIp = host(deployUrl || '', host(loreUrl, ''))
        deployPort = port(deployUrl || '', '7799')
        lorePort = port(loreUrl || '', '41337')
        autoRestoreProject = await api.getAutoRestoreProject()
        autoConnect = await api.getAutoConnect()
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
    const ip = serverIp.trim()
    return ip ? `lore://${ip}:${lorePort.trim() || '41337'}` : ''
  }

  async function testConnection() {
    const url = deployUrl()
    const ip = serverIp.trim()
    testing = true
    connectStatus = 'none'
    loreStatus = 'none'
    // 部署服务器 + loreserver 双状态探测
    if (url) {
      try {
        const ok = await api.testConnection(url, deployToken || null)
        connectStatus = ok ? 'ok' : 'fail'
      } catch {
        connectStatus = 'fail'
      }
    } else {
      connectStatus = 'fail'
    }
    if (ip) {
      try {
        loreStatus = (await api.testLoreConnection(ip)) ? 'ok' : 'fail'
      } catch {
        loreStatus = 'fail'
      }
    } else {
      loreStatus = 'fail'
    }
    testing = false
  }

  async function save() {
    saving = true
    let switched = false
    try {
      // 选中已安装版本且与当前使用版本不同 → 保存时切换当前 Blender 版本
      if (selectedVersion && isInstalled(selectedVersion) && selectedVersion !== currentBlenderVersion) {
        await api.setCurrentBlenderVersion(selectedVersion)
        currentBlenderVersion = selectedVersion
        switched = true
      }
      await api.setDeployToken(deployToken)
      await api.setDeployServerUrl(deployUrl())
      await api.setLoreServerUrl(loreUrl())
      await api.setAutoRestoreProject(autoRestoreProject)
      await api.setAutoConnect(autoConnect)
      await api.setCloseAction(closeAction === 'tray')
      savedFeedback = true
      switchNotice = switched ? `当前版本已切换（Blender ${selectedVersion}）` : ''
      setTimeout(() => { savedFeedback = false; switchNotice = '' }, 2500)
    } catch { /* ignore */ }
    saving = false
  }

  interface BlenderProgress {
    phase: string
    message: string
    percent: number
    bytes_downloaded: number
    bytes_total: number
  }
</script>

<div class="dialog-overlay" onclick={onClose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog settings-panel" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>设置</h3>

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

    <div class="section">
      <div class="section-title">服务器</div>
      <div class="field">
        <label for="sd-server-ip">服务器 IP</label>
        <input id="sd-server-ip" type="text" bind:value={serverIp} placeholder="192.168.0.100" />
      </div>
      <div class="field">
        <span class="port-head">端口</span>
        <div class="input-row">
          <span class="port-label">部署</span>
          <input class="port-input" type="text" bind:value={deployPort} placeholder="7799" aria-label="部署端口" />
          <span class="port-label">loreserver</span>
          <input class="port-input" type="text" bind:value={lorePort} placeholder="41337" aria-label="loreserver 端口" />
          <button class="connect-btn" onclick={testConnection} disabled={testing || !serverIp.trim()}>
            {testing ? '测试中...' : '测试连接'}
          </button>
        </div>
        {#if connectStatus !== 'none' || loreStatus !== 'none'}
          <span class="conn-result">
            部署 {connectStatus === 'ok' ? '✅' : connectStatus === 'fail' ? '❌' : '—'}
            · lore {loreStatus === 'ok' ? '✅' : loreStatus === 'fail' ? '❌' : '—'}
          </span>
        {/if}
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
    </div>

    <div class="section">
      <div class="section-title">通用</div>
      <div class="field toggle-field">
        <label>
          <input type="checkbox" bind:checked={autoRestoreProject} />
          启动时自动打开上次的项目
        </label>
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

    <div class="actions">
      <button onclick={onClose}>取消</button>
      <button class="accent" onclick={save} disabled={saving} title={switchNotice || ''}>{saving ? '保存中…' : switchNotice ? '已切换 ✓' : savedFeedback ? '已保存 ✓' : '保存'}</button>
    </div>
  </div>
</div>

<style>
  .settings-panel {
    width: 520px;
    max-height: 80vh;
    overflow-y: auto;
  }
  h3 { margin-bottom: 16px; font-size: 16px; }
  .section { margin-bottom: 20px; padding-bottom: 16px; border-bottom: 1px solid var(--border); }
  .section:last-of-type { border-bottom: none; margin-bottom: 0; }
  .section-title { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.8px; color: var(--text-dim); margin-bottom: 10px; }
  .field { margin-bottom: 14px; }
  .field:last-child { margin-bottom: 0; }
  .field label { display: block; font-size: 12px; color: var(--text-dim); margin-bottom: 4px; }
  .field input[type="text"], .field input[type="password"] { width: 100%; }
  .hint { display: block; font-size: 10px; color: var(--text-dim); margin-top: 3px; }
  .dl-row { display: flex; align-items: center; gap: 10px; margin-top: 8px; }
  .dl-row button { font-size: 12px; padding: 5px 14px; }
  .dl-row .del {
    background: color-mix(in srgb, var(--error) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--error) 45%, transparent);
    color: var(--error);
  }
  .dl-row .del.confirm {
    background: var(--error);
    border-color: var(--error);
    color: #fff;
    font-weight: 600;
  }
  .dl-row .del:disabled { opacity: 0.5; }
  .dl-row select, .sd-select { width: 200px; font-size: 12px; padding: 5px 8px; background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 6px; }
  .versions-loading { font-size: 11px; color: var(--text-dim); }

  .progress-bar-container {
    width: 100%; height: 6px;
    background: var(--border); border-radius: 3px;
    margin-top: 10px; overflow: hidden;
    position: relative;
  }
  .progress-bar {
    height: 100%; background: var(--accent);
    border-radius: 3px;
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
    font-size: 11px; color: var(--text-dim);
    margin-top: 4px;
  }
  .percent { font-weight: 600; }

  .dl-status { font-size: 11px; color: var(--text-dim); margin-top: 4px; display: block; }
  .dl-status.success { color: var(--success); }
  .dl-status.error { color: var(--error); }
  .toggle-field label { display: flex; align-items: center; gap: 8px; font-size: 13px; cursor: pointer; }
  .toggle-field input[type="checkbox"] { width: auto; }
.input-row { display: flex; align-items: center; gap: 8px; }
.input-row input { flex: 1; }
.port-label { font-size: 11px; color: var(--text-dim); white-space: nowrap; }
.port-head { font-size: 12px; color: var(--text); display: block; margin-bottom: 4px; }
.port-input { flex: 0 0 70px !important; width: 70px; }
.connect-btn { font-size: 11px; padding: 5px 12px; border-radius: 6px; cursor: pointer; background: var(--surface); border: 1px solid var(--border); color: var(--text); white-space: nowrap; }
  .connect-btn:hover:not(:disabled) { background: var(--surface2); }
  .connect-btn:disabled { opacity: 0.4; cursor: default; }
.conn-result { display: block; font-size: 11px; margin-top: 4px; color: var(--text-dim); }
  .actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 20px; }
</style>