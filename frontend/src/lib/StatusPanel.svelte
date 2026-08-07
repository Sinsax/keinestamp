<script lang="ts">
  import { parseStatus, type StatusFile } from './statusParse'
  import StatusTreeItem from './StatusTreeItem.svelte'
  import { buildFileTree, type FileTreeNode } from './fileTree'
  import { openFolder, addToGitignore } from '../api'

  let {
    statusOutput,
    onRefresh,
    activePath = '',
    selection = null,
    onSelectionChange = (_sel: string[] | null) => {},
  }: {
    statusOutput: string
    onRefresh: () => void
    activePath?: string
    selection?: string[] | null
    onSelectionChange?: (sel: string[] | null) => void
  } = $props()

  const files = $derived(parseStatus(statusOutput))
  const statusMap = $derived(Object.fromEntries(files.map((f) => [f.path, f])) as Record<string, StatusFile>)
  const tree = $derived(buildFileTree(files.map((f) => f.path)))
  const fileSet = $derived(new Set(files.map((f) => f.path)))
  const selectionSet = $derived(selection === null ? null : new Set(selection))
  const selectedCount = $derived(selection === null ? files.length : selection.filter((p) => fileSet.has(p)).length)
  const allSelected = $derived(selection === null || selectedCount === files.length)

  const dirFilesMap = $derived.by(() => {
    const map: Record<string, string[]> = {}
    const walk = (nodes: FileTreeNode[]) => {
      for (const n of nodes) {
        if (n.children.length === 0) continue
        const collected: string[] = []
        const collect = (nn: FileTreeNode) => {
          if (nn.children.length === 0) collected.push(nn.path)
          else nn.children.forEach(collect)
        }
        collect(n)
        map[n.path] = collected
        walk(n.children)
      }
    }
    walk(tree)
    return map
  })

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
    return selectionSet === null || selectionSet.has(path)
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

  function dirState(node: FileTreeNode): 'all' | 'none' | 'partial' {
    const fs = dirFilesMap[node.path] || []
    if (selection === null) return fs.length === 0 ? 'none' : 'all'
    let count = 0
    for (const p of fs) if (selection.includes(p)) count++
    if (count === 0) return 'none'
    if (count === fs.length) return 'all'
    return 'partial'
  }

  function toggleDir(node: FileTreeNode) {
    const fs = dirFilesMap[node.path] || []
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

  let contextMenu = $state<{ x: number; y: number; path: string; isDir: boolean } | null>(null)

  function openContextMenu(path: string, isDir: boolean, x: number, y: number) {
    contextMenu = { x, y, path, isDir }
  }

  function closeContextMenu() {
    contextMenu = null
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') closeContextMenu()
  }

  async function menuOpenFolder() {
    const menu = contextMenu
    closeContextMenu()
    if (!menu) return
    const rel = menu.path
    const dir = menu.isDir ? rel : rel.split('/').slice(0, -1).join('/')
    const full = activePath ? `${activePath}\\${dir.replace(/\//g, '\\')}` : ''
    try {
      await openFolder(full)
    } catch (e) {
      console.error('open folder failed', e)
    }
  }

  async function menuCopyPath() {
    const menu = contextMenu
    closeContextMenu()
    if (!menu) return
    const full = activePath ? `${activePath}\\${menu.path.replace(/\//g, '\\')}` : menu.path
    await navigator.clipboard.writeText(full)
  }

  async function menuCopyName() {
    const menu = contextMenu
    closeContextMenu()
    if (!menu) return
    const name = menu.path.split('/').pop() || menu.path
    await navigator.clipboard.writeText(name)
  }

  async function menuIgnore() {
    const menu = contextMenu
    closeContextMenu()
    if (!menu) return
    const rule = menu.isDir ? `${menu.path}/` : menu.path
    try {
      await addToGitignore(activePath, rule)
      onRefresh()
    } catch (e) {
      console.error('add to gitignore failed', e)
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

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
    <div class="sp-list" oncontextmenu={(e) => e.preventDefault()}>
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
          onContextMenu={openContextMenu}
        />
      {/each}
    </div>
  {/if}

  {#if contextMenu}
    <div class="ctx-backdrop" onclick={closeContextMenu} oncontextmenu={(e) => { e.preventDefault(); closeContextMenu() }}></div>
    <div class="ctx-menu" style="left: {contextMenu.x}px; top: {contextMenu.y}px">
      <button onclick={menuOpenFolder}>打开所在文件夹</button>
      <button onclick={menuCopyPath}>复制路径</button>
      <button onclick={menuCopyName}>复制名称</button>
      <div class="ctx-sep"></div>
      <button class="ctx-danger" onclick={menuIgnore}>加入忽略</button>
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
  :global(.sp-item) {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    margin-bottom: 1px;
    font-size: 13px;
    line-height: 1.3;
    border-radius: 6px;
    transition: background 0.12s var(--ease);
  }
  :global(.sp-item:hover) { background: var(--surface); }
  :global(.sp-item.sp-dir) { cursor: pointer; }
  :global(.sp-check) {
    width: 14px;
    height: 14px;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
    flex-shrink: 0;
  }
  :global(.sp-icon) { font-size: 12px; width: 18px; text-align: center; flex-shrink: 0; }
  :global(.sp-ext-badge) {
    font-size: 9px;
    font-weight: 600;
    background: var(--tint-soft);
    padding: 1px 6px;
    border-radius: 4px;
    color: var(--tint);
    flex-shrink: 0;
    letter-spacing: 0.3px;
  }
  :global(.sp-path) {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 12px;
  }
  :global(.sp-label) {
    font-size: 10px;
    font-weight: 500;
    text-transform: uppercase;
    flex-shrink: 0;
  }
  .ctx-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
  }
  .ctx-menu {
    position: fixed;
    z-index: 91;
    min-width: 150px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: var(--shadow-md);
    padding: 4px;
    font-size: 12px;
  }
  .ctx-menu button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 6px 10px;
    background: none;
    border: none;
    border-radius: 5px;
    color: var(--text);
    cursor: pointer;
    font-size: 12px;
    transition: background 0.1s var(--ease), color 0.1s var(--ease);
  }
  .ctx-menu button:hover {
    background: var(--surface2);
    color: var(--accent);
  }
  .ctx-menu .ctx-danger {
    color: var(--error);
  }
  .ctx-menu .ctx-danger:hover {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .ctx-sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
</style>
