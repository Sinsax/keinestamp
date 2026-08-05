<script lang="ts">
  import { onMount } from 'svelte'
  import * as api from '../api'
  import { parseStatus, type StatusFile } from './statusParse'

  let {
    path = '',
    onConfirm,
    onCancel,
    lastTag = '',
  }: {
    path?: string
    onConfirm: (msg: string, tag: string | null, noPreview: boolean, files: string[] | null) => Promise<boolean>
    onCancel: () => void
    lastTag?: string
  } = $props()

  const TAG_PRESETS = ['建模', 'UV拆分', '绑定', '贴图', '修复']

  let message = $state('')
  let tag = $state('')
  let customTag = $state('')
  let useCustom = $state(false)
  let previewEnabled = $state(true)
  let statusFiles = $state<StatusFile[]>([])
  let checked = $state<Record<string, boolean>>({})
  let loadingFiles = $state(false)
  let hint = $state('')

  let selectedFiles = $derived(statusFiles.filter((f) => checked[f.path]).map((f) => f.path))
  let allChecked = $derived(statusFiles.length > 0 && selectedFiles.length === statusFiles.length)
  let hasFiles = $derived(statusFiles.length > 0)

  async function refreshFiles() {
    if (!path) {
      statusFiles = []
      return
    }
    loadingFiles = true
    try {
      const s = await api.status(path)
      statusFiles = parseStatus(s.output)
      const next: Record<string, boolean> = {}
      for (const f of statusFiles) next[f.path] = true
      checked = next
    } catch {
      statusFiles = []
    }
    loadingFiles = false
  }

  onMount(() => { refreshFiles() })

  function toggleAll() {
    const nextAll = !allChecked
    const next: Record<string, boolean> = {}
    for (const f of statusFiles) next[f.path] = nextAll
    checked = next
  }

  function effectiveTag(): string | null {
    if (!tag && !useCustom) return null
    if (useCustom) return customTag.trim() || null
    return tag || null
  }

  async function handleConfirm() {
    if (!message.trim()) return
    const filesArg = hasFiles && !allChecked ? selectedFiles : null
    const ok = await onConfirm(message.trim(), effectiveTag(), !previewEnabled, filesArg)
    if (!ok) return
    await refreshFiles()
    if (statusFiles.length > 0) {
      message = ''
      hint = `还有 ${statusFiles.length} 个文件未提交，可继续分批记录`
    } else {
      onCancel()
    }
  }

  $effect(() => {
    if (lastTag) {
      if (TAG_PRESETS.includes(lastTag)) {
        tag = lastTag
      } else {
        customTag = lastTag
        useCustom = true
      }
    }
  })

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) handleConfirm()
    if (e.key === 'Escape') onCancel()
  }

  function selectPreset(p: string) {
    if (tag === p) {
      tag = ''
    } else {
      tag = p
      useCustom = false
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="dialog-overlay" onclick={onCancel} role="dialog">
  <div class="dialog commit-dialog" onclick={(e: MouseEvent) => e.stopPropagation()} role="document">
    <h3>记录进度</h3>
    <div class="field">
      <label>记录说明</label>
      <textarea
        bind:value={message}
        rows={3}
        placeholder="描述本次修改..."
      ></textarea>
    </div>
    <div class="field">
      <label>标签（可选）</label>
      <div class="tag-row">
        {#each TAG_PRESETS as p}
          <button
            class="tag-btn"
            class:active={tag === p && !useCustom}
            onclick={() => selectPreset(p)}
          >{p}</button>
        {/each}
        <button
          class="tag-btn custom-btn"
          class:active={useCustom}
          onclick={() => { useCustom = true; tag = ''; }}
        >自定义</button>
      </div>
      {#if useCustom}
        <input
          class="custom-input"
          type="text"
          bind:value={customTag}
          placeholder="输入自定义标签..."
        />
      {/if}
    </div>
    {#if hasFiles}
      <div class="field">
        <label class="file-select-header">
          <input type="checkbox" checked={allChecked} onchange={toggleAll} />
          文件选择（{selectedFiles.length}/{statusFiles.length}）
        </label>
        <div class="file-list">
          {#if loadingFiles}
            <div class="file-hint">加载中...</div>
          {:else}
            {#each statusFiles as f}
              <label class="file-item">
                <input type="checkbox" bind:checked={checked[f.path]} />
                <span class="file-status">{f.x !== ' ' ? f.x : f.y || '·'}</span>
                <span class="file-path">{f.path}</span>
              </label>
            {/each}
          {/if}
        </div>
        {#if !allChecked}
          <div class="file-hint">提交所选文件，其余文件保持未提交，可继续分批记录</div>
        {/if}
        {#if hint}
          <div class="file-hint highlight">{hint}</div>
        {/if}
      </div>
    {/if}
    <div class="field row-field">
      <label>3D 预览</label>
      <input type="checkbox" bind:checked={previewEnabled} />
    </div>
    <div class="actions">
      <button onclick={onCancel}>取消</button>
      <button onclick={handleConfirm} class="accent" disabled={!message.trim()}>
        {hasFiles && !allChecked ? `记录所选 (${selectedFiles.length})` : '记录 (Ctrl+Enter)'}
      </button>
    </div>
  </div>
</div>

<style>
  :global(.commit-dialog) {
    width: 460px;
  }
  .tag-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .tag-btn {
    font-size: 12px;
    padding: 4px 12px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-dim);
    cursor: pointer;
    transition: all 0.15s var(--ease);
  }
  .tag-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .tag-btn.active {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
    font-weight: 600;
  }
  .custom-btn.active {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
  }
  .custom-input {
    margin-top: 6px;
    width: 100%;
    padding: 4px 8px;
    font-size: 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
  }
  .row-field {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .row-field label {
    margin-bottom: 0 !important;
  }
  .row-field input[type="checkbox"] {
    width: 14px;
    height: 14px;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .file-select-header {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    font-weight: 600;
  }
  .file-select-header input[type="checkbox"] {
    width: 13px;
    height: 13px;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .file-list {
    max-height: 140px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 4px 6px;
    margin-top: 4px;
    background: var(--bg);
  }
  .file-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 12px;
    transition: background 0.1s;
  }
  .file-item:hover {
    background: var(--surface2);
  }
  .file-item input[type="checkbox"] {
    width: 13px;
    height: 13px;
    accent-color: var(--accent);
    cursor: pointer;
    flex-shrink: 0;
  }
  .file-status {
    width: 16px;
    text-align: center;
    font-weight: 700;
    font-size: 12px;
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .file-path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 11px;
  }
  .file-hint {
    font-size: 10px;
    color: var(--text-dim);
    margin-top: 4px;
  }
  .file-hint.highlight {
    color: var(--accent);
    font-weight: 600;
  }
</style>
