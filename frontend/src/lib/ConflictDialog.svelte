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

  let choices = $state<Record<string, 'current' | 'latest'>>({})
</script>

<div class="dialog-overlay" onclick={onCancel} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog conflict-dialog" onclick={(e: MouseEvent) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>恢复冲突</h3>
    <p class="conflict-hint">以下文件在回退期间被修改且与回退前状态冲突，请为每个文件选择保留的版本：</p>
    <p class="conflict-sub">
      回退期间修改 = 回退状态下你继续编辑的内容；回退前状态 = 回退前暂存的最新进度
    </p>
    {#each conflicts as f}
      <div class="conflict-row">
        <span class="conflict-path" title={f}>{f}</span>
        <div class="conflict-opts">
          <button class:active={choices[f] === 'current'} onclick={() => (choices[f] = 'current')}>回退期间修改</button>
          <button class:active={choices[f] !== 'current'} onclick={() => (choices[f] = 'latest')}>回退前状态</button>
        </div>
      </div>
    {/each}
    <div class="actions">
      <button onclick={onCancel}>取消</button>
      <button
        class="accent"
        disabled={busy}
        onclick={() => onResolve(conflicts.map((f) => [f, choices[f] ?? 'latest'] as RollbackChoice))}
      >
        {busy ? '处理中...' : '完成恢复'}
      </button>
    </div>
  </div>
</div>

<style>
  .conflict-hint {
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 6px;
  }
  .conflict-sub {
    font-size: 11px;
    color: var(--text-dim);
    opacity: 0.8;
    margin-bottom: 12px;
  }
  .conflict-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    margin-bottom: 8px;
  }
  .conflict-path {
    font-size: 12px;
    word-break: break-all;
    flex: 1;
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
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
  }
  .conflict-opts button.active {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
</style>
