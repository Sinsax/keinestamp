<script lang="ts">
  import { parseStatus, type StatusFile } from './statusParse'

  let {
    statusOutput,
    onRefresh,
    selection = null,
    onSelectionChange = (_sel: string[] | null) => {},
  }: {
    statusOutput: string
    onRefresh: () => void
    selection?: string[] | null
    onSelectionChange?: (sel: string[] | null) => void
  } = $props()

  const files = $derived(parseStatus(statusOutput))
  const selectedCount = $derived(selection === null ? files.length : selection.filter((p) => files.some((f) => f.path === p)).length)
  const allSelected = $derived(selection === null || selectedCount === files.length)

  function isChecked(path: string): boolean {
    return selection === null || selection.includes(path)
  }

  function toggleFile(path: string) {
    if (selection === null) {
      const next = files.filter((f) => f.path !== path).map((f) => f.path)
      onSelectionChange(next)
    } else {
      const next = selection.includes(path)
        ? selection.filter((p) => p !== path)
        : [...selection, path]
      if (next.length === files.length) {
        onSelectionChange(null)
      } else {
        onSelectionChange(next)
      }
    }
  }

  function selectAll() {
    onSelectionChange(null)
  }

  function statusLabel(x: string, y: string): string {
    if (x !== ' ') return 'staged'
    if (y === 'M') return 'modified'
    if (y === 'A') return 'added'
    if (y === 'D') return 'deleted'
    if (y === '?') return 'untracked'
    return ''
  }

  function statusColor(x: string, y: string): string {
    if (x !== ' ') {
      if (x === 'M') return 'var(--accent)'
      if (x === 'A') return 'var(--success)'
      if (x === 'D') return 'var(--error)'
    }
    if (y === 'M') return 'var(--warning)'
    if (y === 'A') return 'var(--success)'
    if (y === 'D') return 'var(--error)'
    if (y === '?') return 'var(--text-dim)'
    return 'var(--text-dim)'
  }

  function iconType(x: string, y: string): string {
    if (x === 'A' || y === 'A') return 'added'
    if (x === 'D' || y === 'D') return 'deleted'
    if ((x !== ' ' || y === 'M')) return 'modified'
    if (y === '?') return 'untracked'
    return 'unknown'
  }

  function ext(path: string): string {
    const i = path.lastIndexOf('.')
    return i >= 0 ? path.slice(i + 1).toUpperCase() : 'FILE'
  }
</script>

<div class="status-panel">
  <div class="sp-header">
    <span class="sp-title">Changes</span>
    <span class="sp-count">{selectedCount}/{files.length} file{files.length !== 1 ? 's' : ''}</span>
    <div class="sp-spacer"></div>
    {#if !allSelected}
      <button class="sp-select-all" onclick={selectAll} title="全部选中">全选</button>
    {/if}
    <button class="sp-refresh" onclick={onRefresh} title="Refresh status">
      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 3a5 5 0 014.546 2.916.5.5 0 01-.908.417A4 4 0 108 11.5a4.002 4.002 0 003.392-1.87.5.5 0 11.854.52A5 5 0 113 8a5 5 0 015-5z"/><path d="M11.5 3.5A.5.5 0 0112 4v2.5a.5.5 0 01-.5.5H9a.5.5 0 010-1h1.802A4.003 4.003 0 004 8a.5.5 0 01-1 0 5 5 0 015-5 .5.5 0 01.5.5z"/></svg>
    </button>
  </div>

  {#if files.length === 0}
    <div class="sp-empty">
      <div class="sp-empty-icon"><svg viewBox="0 0 16 16" width="28" height="28" fill="currentColor" opacity="0.5"><path d="M13.854 3.646a.5.5 0 010 .708l-7 7a.5.5 0 01-.708 0l-3.5-3.5a.5.5 0 11.708-.708L6.5 10.293l6.646-6.647a.5.5 0 01.708 0z"/></svg></div>
      <div>无变更 — 工作区干净</div>
    </div>
  {:else}
    <div class="sp-list">
      {#each files as f}
        <div class="sp-item">
          <input
            type="checkbox"
            class="sp-check"
            checked={isChecked(f.path)}
            onchange={() => toggleFile(f.path)}
            title="取消勾选可单独提交其余文件"
          />
          <span class="sp-icon" style="color: {statusColor(f.x, f.y)}">
            {#if iconType(f.x, f.y) === 'added'}
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 2a.5.5 0 01.5.5v5h5a.5.5 0 010 1h-5v5a.5.5 0 01-1 0v-5h-5a.5.5 0 010-1h5v-5A.5.5 0 018 2z"/></svg>
            {:else if iconType(f.x, f.y) === 'deleted'}
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M4 8a.5.5 0 01.5-.5h7a.5.5 0 010 1h-7A.5.5 0 014 8z"/></svg>
            {:else if iconType(f.x, f.y) === 'modified'}
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 016 13.5V13h-.5a.5.5 0 01-.5-.5V12h-.5a.5.5 0 01-.5-.5V11h-.5a.5.5 0 01-.5-.5V10h-.5a.499.499 0 01-.175-.032l-.179.178a.5.5 0 00-.11.168l-2 5a.5.5 0 00.65.65l5-2a.5.5 0 00.168-.11l.178-.178z"/></svg>
            {:else}
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 15A7 7 0 118 1a7 7 0 010 14zm0 1A8 8 0 108 0a8 8 0 000 16z"/><path d="M5.255 5.786a.237.237 0 00.241.247h.825c.138 0 .248-.113.266-.25.09-.656.54-1.134 1.342-1.134.686 0 1.314.343 1.314 1.168 0 .635-.374.927-.965 1.371-.673.489-1.206 1.06-1.168 1.987l.003.217a.25.25 0 00.25.246h.811a.25.25 0 00.25-.25v-.105c0-.718.273-.927 1.01-1.486.609-.463 1.244-.977 1.244-2.056 0-1.511-1.276-2.241-2.673-2.241-1.267 0-2.655.59-2.75 2.286zm1.557 5.763c0 .533.425.927 1.01.927.609 0 1.028-.394 1.028-.927 0-.552-.42-.94-1.029-.94-.584 0-1.009.388-1.009.94z"/></svg>
            {/if}
          </span>
          <span class="sp-ext-badge">{ext(f.path)}</span>
          <span class="sp-path">{f.path}</span>
          <span class="sp-label" style="color: {statusColor(f.x, f.y)}">{statusLabel(f.x, f.y)}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .status-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .sp-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .sp-title {
    font-size: 13px;
    font-weight: 600;
  }
  .sp-count {
    font-size: 11px;
    color: var(--text-dim);
    background: var(--surface2);
    padding: 1px 8px;
    border-radius: 999px;
  }
  .sp-spacer { flex: 1; }
  .sp-select-all {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 10px;
    padding: 2px 8px;
    border-radius: 6px;
    cursor: pointer;
    transition: color 0.12s, border-color 0.12s;
  }
  .sp-select-all:hover {
    color: var(--accent);
    border-color: var(--accent);
  }
  .sp-check {
    width: 13px;
    height: 13px;
    accent-color: var(--accent);
    cursor: pointer;
    flex-shrink: 0;
  }
  .sp-refresh {
    background: none;
    border: none;
    font-size: 16px;
    padding: 2px 6px;
    cursor: pointer;
    color: var(--text-dim);
    border-radius: 6px;
    transition: background 0.12s, color 0.12s;
  }
  .sp-refresh:hover { background: var(--surface2); color: var(--accent); }
  .sp-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    gap: 8px;
    color: var(--text-dim);
    font-size: 13px;
  }
  .sp-empty-icon { font-size: 28px; opacity: 0.5; }
  .sp-list {
    flex: 1;
    overflow-y: auto;
    padding: 6px 8px;
  }
  .sp-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 10px;
    margin-bottom: 2px;
    font-size: 13px;
    border-radius: 8px;
    transition: background 0.12s var(--ease), box-shadow 0.12s var(--ease);
  }
  .sp-item:hover { background: var(--surface); box-shadow: var(--shadow-sm); }
  .sp-icon { font-size: 12px; width: 18px; text-align: center; flex-shrink: 0; }
  .sp-ext-badge {
    font-size: 9px;
    font-weight: 600;
    background: var(--ice-soft);
    padding: 1px 6px;
    border-radius: 4px;
    color: var(--blue);
    flex-shrink: 0;
    letter-spacing: 0.3px;
  }
  .sp-path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 12px;
  }
  .sp-label {
    font-size: 10px;
    font-weight: 500;
    text-transform: uppercase;
    flex-shrink: 0;
  }
</style>
