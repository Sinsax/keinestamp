<script lang="ts">
  import type { ProjectType } from '../types'

  let {
    projectType,
    onCommit,
    onExport,
    onRevert,
  }: {
    projectType: ProjectType | null
    onCommit: () => void
    onExport: () => void
    onRevert: () => void
  } = $props()

  const initialized = $derived(projectType === 'initialized')
</script>

<div class="action-bar">
  <button onclick={onCommit} disabled={!initialized}>
    <span class="ab-label">保存</span>
    <span class="ab-hint">commit</span>
  </button>

  <button onclick={onExport} disabled={!initialized}>
    <span class="ab-label">导出</span>
    <span class="ab-hint">export</span>
  </button>

  <button onclick={onRevert} disabled={!initialized}>
    <span class="ab-label">撤回</span>
    <span class="ab-hint">revert</span>
  </button>
</div>

<style>
  .action-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    height: var(--action-h);
    padding: 0 8px;
    background: var(--surface);
    border-top: 1px solid var(--border);
  }
  .action-bar button {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
    font-size: 11px;
    padding: 4px 14px;
    border-radius: 6px;
    cursor: pointer;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    transition: background 0.12s, border-color 0.12s;
  }
  .action-bar button:hover:not(:disabled) {
    background: var(--surface2);
    border-color: var(--surface3);
  }
  .action-bar button:disabled {
    opacity: 0.35;
    cursor: default;
    pointer-events: none;
  }
  .ab-label {
    font-size: 13px;
    font-weight: 600;
    line-height: 1.2;
  }
  .ab-hint {
    font-size: 9px;
    color: var(--text-dim);
    line-height: 1.1;
  }
  .spacer { flex: 1; }
</style>