<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import * as api from '../api'

  let {
    path = '',
    onClose,
    cachedVersions = [],
  }: {
    path?: string
    onClose: () => void
    cachedVersions?: string[]
  } = $props()

  let webRemote = $state('')
  let deployToken = $state('')
  let autoConnect = $state(false)
  let selectedVersion = $state('')
  let versions = $state<string[]>([])
  let installedVersions = $state<string[]>([])
  let versionsLoading = $state(false)
  let saving = $state(false)
  let downloading = $state(false)
  let dlPhase = $state('')
  let dlMessage = $state('')
  let dlPercent = $state(0)
  let testing = $state(false)
  let connectStatus = $state<'none' | 'ok' | 'fail'>('none')
  let savedFeedback = $state(false)

  $effect(() => {
    if (cachedVersions.length > 0 && versions.length === 0) {
      versions = [...cachedVersions]
      if (!selectedVersion) selectedVersion = versions[0]
    }
  })

  onMount(() => {
    (async () => {
      try {
        deployToken = await api.getDeployToken()
        webRemote = await api.getDeployServerUrl()
        autoConnect = await api.getAutoConnect()
      } catch { /* ignore */ }
    })()

    if (versions.length > 0 && !selectedVersion) {
      selectedVersion = versions[0]
    }
    if (versions.length === 0 && cachedVersions.length === 0) {
      loadVersions()
    }
    loadInstalled()

    const unlistenPromise = listen<BlenderProgress>('blender-download-progress', (event) => {
      dlPhase = event.payload.phase
      dlMessage = event.payload.message
      dlPercent = event.payload.percent
    })
    return () => { unlistenPromise.then(fn => fn()) }
  })

  async function loadVersions() {
    versionsLoading = true
    try {
      versions = await api.listBlenderVersions()
      if (versions.length > 0 && !selectedVersion) {
        selectedVersion = versions[0]
      }
    } catch { /* ignore */ }
    versionsLoading = false
  }

  async function loadInstalled() {
    try {
      installedVersions = await api.listInstalledBlenderVersions()
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

  async function testConnection() {
    if (!webRemote) {
      connectStatus = 'fail'
      return
    }
    testing = true
    connectStatus = 'none'
    try {
      const ok = await api.testConnection(webRemote, deployToken || null)
      connectStatus = ok ? 'ok' : 'fail'
    } catch {
      connectStatus = 'fail'
    }
    testing = false
  }

  async function save() {
    saving = true
    try {
      await api.setDeployToken(deployToken)
      await api.setDeployServerUrl(webRemote)
      await api.setAutoConnect(autoConnect)
      savedFeedback = true
      setTimeout(() => savedFeedback = false, 2000)
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

<div class="overlay" onclick={onClose} role="dialog">
  <div class="panel" onclick={(e) => e.stopPropagation()} role="document">
    <h3>设置</h3>

    <div class="section">
      <div class="section-title">Blender</div>
      <div class="field">
        <span class="hint">Keinestamp 自带独立 Blender，不影响系统已安装的版本，无插件冲突风险</span>
        <div class="dl-row">
          {#if versionsLoading}
            <span class="versions-loading">加载版本列表...</span>
          {:else}
            <select bind:value={selectedVersion} disabled={downloading}>
              {#each versions as v}
                <option value={v}>
                  {v}{v === versions[0] ? ' (最新)' : ''}{isInstalled(v) ? ' ✓' : ''}
                </option>
              {/each}
            </select>
          {/if}
          <button class="accent" onclick={downloadBlender} disabled={downloading || !selectedVersion || isInstalled(selectedVersion)}>
            {downloading ? '下载中...' : isInstalled(selectedVersion) ? '已安装' : '下载 Blender'}
          </button>
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
        <div class="section-title">服务器连接</div>
        <div class="field">
          <label>服务器地址</label>
          <div class="input-row">
            <input type="text" bind:value={webRemote} placeholder="http://192.168.0.100:7799" />
            <button class="connect-btn" onclick={testConnection} disabled={testing || !webRemote}>
              {testing ? '测试中...' : '测试连接'}
            </button>
          </div>
          <span class="hint">Keinestamp Server 监听地址（全局设置，跟随软件）</span>
          {#if connectStatus === 'ok'}
            <span class="conn-result ok">✅ 连接成功</span>
          {:else if connectStatus === 'fail'}
            <span class="conn-result fail">❌ 连接失败</span>
          {/if}
        </div>
        <div class="field">
          <label>部署令牌（Deploy Token）</label>
          <input type="password" bind:value={deployToken} placeholder="可选" />
          <span class="hint">用于 App 上传认证（Bearer Token），保存在本机</span>
        </div>
        <div class="field toggle-field">
          <label>
            <input type="checkbox" bind:checked={autoConnect} />
            打开项目时自动测试连接（软件级设置）
          </label>
        </div>
      </div>

    <div class="actions">
      <button onclick={onClose}>取消</button>
      <button class="accent" onclick={save} disabled={saving}>{saving ? '保存中…' : savedFeedback ? '已保存 ✓' : '保存'}</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(30, 40, 51, 0.35);
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    animation: fade-in 0.15s var(--ease);
  }
  .panel {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 24px;
    width: 520px;
    max-height: 80vh;
    overflow-y: auto;
    box-shadow: var(--shadow-lg);
    animation: dialog-in 0.2s var(--ease);
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
  .dl-row select { width: 200px; font-size: 12px; padding: 5px 8px; background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 6px; }
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
  .connect-btn { font-size: 11px; padding: 5px 12px; border-radius: 6px; cursor: pointer; background: var(--surface); border: 1px solid var(--border); color: var(--text); white-space: nowrap; }
  .connect-btn:hover:not(:disabled) { background: var(--surface2); }
  .connect-btn:disabled { opacity: 0.4; cursor: default; }
  .conn-result { display: block; font-size: 11px; margin-top: 4px; }
  .conn-result.ok { color: var(--success); }
  .conn-result.fail { color: var(--error); }
  .actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 20px; }
</style>