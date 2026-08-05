<script lang="ts">
  import { parseStatus, type StatusFile } from './statusParse'
  import StatusTreeItem from './StatusTreeItem.svelte'
  import { buildFileTree, type FileTreeNode } from './fileTree'

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
  const statusMap = $derived(Object.fromEntries(files.map((f) => [f.path, f])) as Record<string, StatusFile>)
  const tree = $derived(buildFileTree(files.map((f) => f.path)))
  const selectedCount = $derived(selection === null ? files.length : selection.filter((p) => files.some((f) => f.path === p)).length)
  const allSelected = $derived(selection === null || selectedCount === files.length)

  let collapsedDirs = $state<Set<string>>(new Set())

  function isCollapsed(path: string): boolean {
    return collapsedDirs.has(path)
  }

  function toggleCollapse(path: string) {
    const next = new Set(collapsedDirs)
    if (next.has(path)) next.delete(path)
    else next.add(path)
    collapsedDirs = next
  }

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

  function collectFiles(node: FileTreeNode): string[] {
    const out: string[] = []
    const walk = (n: FileTreeNode) => {
      if (n.children.length === 0) out.push(n.path)
      else n.children.forEach(walk)
    }
    walk(node)
    return out
  }

  function dirState(node: FileTreeNode): 'all' | 'none' | 'partial' {
    const fs = collectFiles(node)
    const sel = selection === null ? files.map((f) => f.path) : selection
    const selSet = new Set(sel)
    let count = 0
    for (const p of fs) if (selSet.has(p)) count++
    if (count === 0) return 'none'
    if (count === fs.length) return 'all'
    return 'partial'
  }

  function toggleDir(node: FileTreeNode) {
    const fs = collectFiles(node)
    const base = selection === null ? files.map((f) => f.path) : [...selection]
    const baseSet = new Set(base)
    const allIn = fs.every((p) => baseSet.has(p))
    const next = allIn
      ? base.filter((p) => !fs.includes(p))
      : Array.from(new Set([...base, ...fs]))
    onSelectionChange(next.length === files.length ? null : next)
  }

  function selectAll() {
    onSelectionChange(null)
  }

  function selectNone() {
    onSelectionChange([])
  }

  function statusOf(path: string): StatusFile | undefined {
    return statusMap[path]
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
    {#if selectedCount > 0}
      <button class="sp-select-all" onclick={selectNone} title="全部取消">全不选</button>
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
      {#each tree as node (node.path)}
        <StatusTreeItem
          {node}
          depth={0}
          {statusOf}
          {isChecked}
          {dirState}
          onToggleFile={toggleFile}
          onToggleDir={toggleDir}
          {isCollapsed}
          onToggleCollapse={toggleCollapse}
        />
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
    gap: 6px;
    padding: 4px 8px;
    margin-bottom: 1px;
    font-size: 13px;
    border-radius: 6px;
    transition: background 0.12s var(--ease);
  }
  .sp-item:hover { background: var(--surface); }
  .sp-item.sp-dir { cursor: pointer; }
  .sp-check {
    width: 13px;
    height: 13px;
    accent-color: var(--accent);
    cursor: pointer;
    flex-shrink: 0;
  }
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
