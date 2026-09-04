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
    background: var(--dialog-overlay);
    backdrop-filter: blur(3px);
    -webkit-backdrop-filter: blur(3px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    animation: fade-in 0.12s var(--ease);
  }
  .conflict-dialog {
    width: 480px;
    max-height: 80vh;
    overflow-y: auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 16px 18px;
    box-shadow: var(--shadow-lg);
  }
  .conflict-hint {
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 10px;
  }
  .conflict-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 5px 0;
    border-bottom: 1px solid var(--border);
  }
  .conflict-path {
    font-family: var(--mono, monospace);
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }
  .conflict-opts {
    display: flex;
    gap: 5px;
    flex-shrink: 0;
  }
  .conflict-opts button {
    font-size: 11px;
    padding: 2px 9px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    background: transparent;
    border: 1px solid var(--border-strong);
    color: var(--text-dim);
    transition: all 0.12s var(--ease);
  }
  .conflict-opts button.active {
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
    margin-top: 14px;
  }
  .actions button {
    font-size: 12px;
    padding: 4px 14px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    background: var(--surface);
    border: 1px solid var(--border-strong);
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
