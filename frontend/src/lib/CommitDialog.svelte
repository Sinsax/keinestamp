<script lang="ts">
  let {
    onConfirm,
    onCancel,
    lastTag = '',
    canRestore = false,
    busy = false,
    progress = '',
  }: {
    onConfirm: (msg: string, tag: string | null, noPreview: boolean) => Promise<boolean>
    onCancel: () => void
    lastTag?: string
    canRestore?: boolean
    busy?: boolean
    progress?: string
  } = $props()

  const TAG_PRESETS = ['建模', 'UV拆分', '绑定', '贴图', '修复']

  let message = $state('')
  let tag = $state('')
  let customTag = $state('')
  let useCustom = $state(false)
  let previewEnabled = $state(true)

  function effectiveTag(): string | null {
    if (!tag && !useCustom) return null
    if (useCustom) return customTag.trim() || null
    return tag || null
  }

  async function handleConfirm() {
    if (busy || !message.trim()) return
    const ok = await onConfirm(message.trim(), effectiveTag(), !previewEnabled)
    if (ok) onCancel()
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
    if (e.key === 'Escape' && !busy) onCancel()
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

<div class="dialog-overlay" onclick={() => { if (!busy) onCancel() }} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog commit-dialog" class:restore-mode={canRestore} onclick={(e: MouseEvent) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>
      {#if canRestore}
        <span class="restore-mark">
          <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M8 1.5a6.5 6.5 0 100 13 6.5 6.5 0 000-13zm0 1.5a5 5 0 110 10 5 5 0 010-10z"/><path d="M7.2 4.6h1.6v4.4H7.2zM7.2 10.4h1.6v1.2H7.2z"/></svg>
        </span>
      {/if}
      记录进度
    </h3>
    {#if canRestore}
      <div class="restore-warn">
        <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 1.5a6.5 6.5 0 100 13 6.5 6.5 0 000-13zm0 1.5a5 5 0 110 10 5 5 0 010-10z"/><path d="M7.2 4.6h1.6v4.4H7.2zM7.2 10.4h1.6v1.2H7.2z"/></svg>
        <span>回退状态下记录将顶替被回退的记录（原记录被新记录取代）</span>
      </div>
    {/if}
    <div class="field">
      <label for="cd-message">记录说明</label>
      <textarea
        id="cd-message"
        bind:value={message}
        rows={3}
        placeholder="描述本次修改..."
      ></textarea>
    </div>
    <div class="field">
      <label for="cd-tag">标签（可选）</label>
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
          id="cd-tag"
          class="custom-input"
          type="text"
          bind:value={customTag}
          placeholder="输入自定义标签..."
        />
      {/if}
    </div>
    <div class="field row-field">
      <label for="cd-preview">3D 预览</label>
      <input id="cd-preview" type="checkbox" bind:checked={previewEnabled} />
    </div>
    <div class="actions">
      <button onclick={onCancel} disabled={busy}>取消</button>
      <button onclick={handleConfirm} class="accent" disabled={busy || !message.trim()}>
        {busy ? (progress ? `记录中...：${progress}` : '记录中...') : '记录 (Ctrl+Enter)'}
      </button>
    </div>
  </div>
</div>

<style>
  :global(.commit-dialog) {
    width: 460px;
  }
  :global(.commit-dialog.restore-mode) {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent), 0 8px 32px color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .restore-mark {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    vertical-align: -2px;
    margin-right: 4px;
    color: var(--accent);
  }
  .restore-warn {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    margin-bottom: 14px;
    background: var(--accent-soft);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
    border-radius: 8px;
    color: var(--accent);
    font-size: 12px;
    font-weight: 600;
    line-height: 1.6;
  }
  .restore-warn svg {
    flex-shrink: 0;
    margin-top: 2px;
  }
  .field textarea {
    resize: none;
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
</style>
