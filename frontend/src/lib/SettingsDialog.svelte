<script lang="ts">
  import { onMount } from 'svelte'
  import { open } from '@tauri-apps/plugin-dialog'
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
  let webPushEnabled = $state(false)
  let blenderPath = $state('')
  let selectedVersion = $state('')
  let versions = $state<string[]>([])
  let versionsLoading = $state(false)
  let saving = $state(false)
  let downloading = $state(false)
  let dlPhase = $state('')
  let dlMessage = $state('')
  let dlPercent = $state(0)
  let hasProject = $derived(!!path)

  $effect(() => {
    if (cachedVersions.length > 0 && versions.length === 0) {
      versions = [...cachedVersions]
      if (!selectedVersion) selectedVersion = versions[0]
    }
  })

  onMount(() => {
    if (hasProject) {
      (async () => {
        try {
          const cfg = await api.readConfig(path)
          webRemote = cfg.web_remote || ''
          webPushEnabled = cfg.web_push_enabled
        } catch { /* ignore */ }
      })()
    }
    (async () => {
      try {
        blenderPath = await api.getBlenderPath()
      } catch { /* ignore */ }
    })()

    if (versions.length > 0 && !selectedVersion) {
      selectedVersion = versions[0]
    }
    if (versions.length === 0) {
      loadVersions()
    }

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

  async function pickBlender() {
    const selected = await open({
      multiple: false,
      title: '选择 Blender 可执行文件',
      filters: [{ name: 'Blender', extensions: ['exe', 'blender', ''] }]
    })
    if (selected) blenderPath = selected
  }

  async function downloadBlender() {
    downloading = true
    dlPhase = 'downloading'
    dlMessage = '正在下载...'
    dlPercent = 0
    try {
      const exePath = await api.downloadBlender(selectedVersion || null)
      blenderPath = exePath
      dlPhase = 'done'
      dlMessage = '下载完成 — 使用自带 Blender，无插件干扰 ✓'
    } catch (e) {
      dlPhase = 'error'
      dlMessage = `下载失败: ${e}`
    }
    downloading = false
  }

  async function save() {
    saving = true
    try {
      await api.setBlenderPath(blenderPath || '')
      if (hasProject) {
        await api.updateConfig(path, 'web_remote', webRemote || null)
        await api.updateConfig(path, 'web_push_enabled', webPushEnabled ? 'true' : 'false')
      }
      onClose()
    } catch { /* ignore */ }
    saving = false
  }

  $effect(() => {
    if (webRemote && !webPushEnabled) {
      webPushEnabled = true
    }
  })

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
        <label>Blender 路径</label>
        <div class="input-row">
          <input type="text" bind:value={blenderPath} placeholder="留空则使用系统 PATH 中的 blender" />
          <button class="browse-btn" onclick={pickBlender}>浏览</button>
        </div>
        <span class="hint">SakiStep 自带独立 Blender，不影响系统已安装的版本，无插件冲突风险</span>
        <div class="dl-row">
          {#if versionsLoading}
            <span class="versions-loading">加载版本列表...</span>
          {:else}
            <select bind:value={selectedVersion} disabled={downloading}>
              {#each versions as v}
                <option value={v}>{v}{v === versions[0] ? ' (最新)' : ''}</option>
              {/each}
            </select>
          {/if}
          <button class="accent" onclick={downloadBlender} disabled={downloading || !selectedVersion}>
            {downloading ? '下载中...' : '下载 Blender'}
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

    {#if hasProject}
      <div class="section">
        <div class="section-title">网页推送</div>
        <div class="field">
          <label>网页仓库地址</label>
          <input type="text" bind:value={webRemote} placeholder="https://gitea.example.com/user/project-web.git" />
          <span class="hint">推送时间线页面到 Gitea 远端仓库，Caddy 代理提供访问</span>
        </div>
        <div class="field toggle-field">
          <label>
            <input type="checkbox" bind:checked={webPushEnabled} />
            提交后自动推送网页
          </label>
        </div>
      </div>
    {/if}

    <div class="actions">
      <button onclick={onClose}>取消</button>
      <button class="accent" onclick={save} disabled={saving}>{saving ? '保存中…' : '保存'}</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.65);
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .panel {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 24px;
    width: 520px;
    max-height: 80vh;
    overflow-y: auto;
    box-shadow: 0 8px 32px rgba(0,0,0,0.4);
  }
  h3 { margin-bottom: 16px; font-size: 16px; }
  .section { margin-bottom: 20px; padding-bottom: 16px; border-bottom: 1px solid var(--border); }
  .section:last-of-type { border-bottom: none; margin-bottom: 0; }
  .section-title { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.8px; color: var(--text-dim); margin-bottom: 10px; }
  .field { margin-bottom: 14px; }
  .field:last-child { margin-bottom: 0; }
  .field label { display: block; font-size: 12px; color: var(--text-dim); margin-bottom: 4px; }
  .field input[type="text"] { width: 100%; }
  .hint { display: block; font-size: 10px; color: var(--text-dim); margin-top: 3px; }
  .input-row { display: flex; gap: 6px; }
  .input-row input { flex: 1; }
  .browse-btn { flex-shrink: 0; padding: 7px 14px; font-size: 12px; }
  .dl-row { display: flex; align-items: center; gap: 10px; margin-top: 8px; }
  .dl-row button { font-size: 12px; padding: 5px 14px; }
  .dl-row select { width: 160px; font-size: 12px; padding: 5px 8px; background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: 6px; }
  .versions-loading { font-size: 11px; color: var(--text-dim); }

  .progress-bar-container {
    width: 100%; height: 6px;
    background: var(--border); border-radius: 3px;
    margin-top: 10px; overflow: hidden;
    position: relative;
  }
  .progress-bar {
    height: 100%; background: #7c3aed;
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
  .dl-status.success { color: #4ade80; }
  .dl-status.error { color: #f87171; }
  .toggle-field label { display: flex; align-items: center; gap: 8px; font-size: 13px; cursor: pointer; }
  .toggle-field input[type="checkbox"] { width: auto; }
  .actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 20px; }
</style>