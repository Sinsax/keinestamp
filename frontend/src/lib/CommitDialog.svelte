<script lang="ts">
  let { onConfirm, onCancel }: { onConfirm: (msg: string, tag: string | null, noPreview: boolean) => void; onCancel: () => void } = $props()

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

  function handleConfirm() {
    if (message.trim()) onConfirm(message.trim(), effectiveTag(), !previewEnabled)
  }

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
    <div class="field row-field">
      <label>3D 预览</label>
      <input type="checkbox" bind:checked={previewEnabled} />
    </div>
    <div class="actions">
      <button onclick={onCancel}>取消</button>
      <button onclick={handleConfirm} class="accent" disabled={!message.trim()}>
        记录 (Ctrl+Enter)
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
    padding: 3px 10px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-dim);
    cursor: pointer;
    transition: all 0.15s;
  }
  .tag-btn:hover {
    border-color: var(--accent);
    color: var(--text);
  }
  .tag-btn.active {
    background: var(--accent);
    color: #1e1e2e;
    border-color: var(--accent);
    font-weight: 600;
  }
  .custom-btn.active {
    background: var(--accent);
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