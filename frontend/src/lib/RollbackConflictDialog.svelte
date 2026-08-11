<script lang="ts">
  import type { RollbackChoice } from '../types'

  let {
    conflicts,
    busy,
    onResolve,
    onCancel,
  }: {
    conflicts: string[]
    busy: boolean
    onResolve: (choices: RollbackChoice[]) => void
    onCancel: () => void
  } = $props()

  let choices = $state<Record<string, 'current' | 'stashed'>>({})
</script>

<div class="dialog-overlay" onclick={onCancel} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog conflict-dialog" onclick={(e: MouseEvent) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>恢复冲突</h3>
    <p class="conflict-hint">以下文件在回退期间被改动，且与回退前暂存的内容冲突，请为每个文件选择保留的版本：</p>
    {#each conflicts as f}
      <div class="conflict-row">
        <span class="conflict-path" title={f}>{f}</span>
        <div class="conflict-opts">
          <button
            class:active={choices[f] !== 'stashed'}
            onclick={() => (choices[f] = 'current')}
          >当前版本改动</button>
          <button
            class:active={choices[f] === 'stashed'}
            onclick={() => (choices[f] = 'stashed')}
          >回退前改动</button>
        </div>
      </div>
    {/each}
    <div class="actions">
      <button onclick={onCancel}>取消</button>
      <button
        class="accent"
        disabled={busy || conflicts.some((f) => choices[f] === undefined)}
        onclick={() => onResolve(conflicts.map((f) => [f, choices[f] === 'stashed' ? 'stashed' : 'current']))}
      >
        {busy ? '恢复中...' : '确定恢复'}
      </button>
    </div>
  </div>
</div>

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, #000 45%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .conflict-dialog {
    width: 480px;
    max-height: 80vh;
    overflow-y: auto;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 18px;
    box-shadow: 0 8px 40px rgba(0, 0, 0, 0.3);
  }
  .conflict-hint {
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 12px;
  }
  .conflict-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .conflict-path {
    font-family: var(--mono, monospace);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }
  .conflict-opts {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .conflict-opts button {
    font-size: 11px;
    padding: 4px 10px;
    border-radius: 6px;
    cursor: pointer;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-dim);
  }
  .conflict-opts button.active {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 16px;
  }
  .actions button {
    font-size: 12px;
    padding: 6px 16px;
    border-radius: 8px;
    cursor: pointer;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
  }
  .actions button.accent {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
