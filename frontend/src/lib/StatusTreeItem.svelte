<script lang="ts">
  import StatusTreeItem from './StatusTreeItem.svelte'
  import type { FileTreeNode } from './fileTree'
  import type { StatusFile } from './statusParse'
  import { statusColor, statusIconType, statusLabel, ext } from './statusParse'

  let {
    node,
    depth = 0,
    statusOf,
    isChecked,
    dirState,
    onToggleFile,
    onToggleDir,
    isCollapsed,
    onToggleCollapse,
    onContextMenu = (_path: string, _isDir: boolean, _x: number, _y: number) => {},
    ignored = false,
    isIgnoredFn = (_path: string) => false,
    dirAllIgnoredFn = (_path: string) => false,
    rollbackChangeSet = new Set<string>(),
    rollbackDeletedSet = new Set<string>(),
    rollbackModifiedSet = new Set<string>(),
    previewMarked = false,
  }: {
    node: FileTreeNode
    depth?: number
    statusOf: (path: string) => StatusFile | undefined
    isChecked: (path: string) => boolean
    dirState: (node: FileTreeNode) => 'all' | 'none' | 'partial'
    onToggleFile: (path: string) => void
    onToggleDir: (node: FileTreeNode) => void
    isCollapsed: (path: string) => boolean
    onToggleCollapse: (path: string) => void
    onContextMenu?: (path: string, isDir: boolean, x: number, y: number) => void
    ignored?: boolean
    isIgnoredFn?: (path: string) => boolean
    dirAllIgnoredFn?: (path: string) => boolean
    rollbackChangeSet?: Set<string>
    rollbackDeletedSet?: Set<string>
    rollbackModifiedSet?: Set<string>
    previewMarked?: boolean
  } = $props()

  let indet = $state(false)
  $effect(() => { indet = dirState(node) === 'partial' })

  const isDir = $derived(node.children.length > 0)
  const status = $derived(isDir ? undefined : statusOf(node.path))
  const rollbackMark = $derived(rollbackChangeSet.has(node.path))
  const rollbackDeleted = $derived(rollbackDeletedSet.has(node.path))
  const rollbackModified = $derived(rollbackModifiedSet.has(node.path))
  // 整行着色：回退改动恒黄、回退删除恒红；普通状态 M/A/D/??
  const stClass = $derived(
    rollbackMark ? 'sp-st-m'
    : rollbackDeleted ? 'sp-st-d'
    : status?.x === 'M' ? 'sp-st-m'
    : status?.x === 'A' ? 'sp-st-a'
    : status?.x === 'D' ? 'sp-st-d'
    : status?.x === '?' ? 'sp-st-u'
    : ''
  )
</script>

{#if isDir}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="sp-item sp-dir" class:sp-ignored={ignored} onclick={() => { if (!ignored) onToggleDir(node) }} oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); onContextMenu(node.path, true, e.clientX, e.clientY) }} title="点击 = 选择/取消目录内所有文件">
    <input
      type="checkbox"
      class="sp-check"
      checked={dirState(node) !== 'none'}
      bind:indeterminate={indet}
      disabled={ignored}
      onclick={(e) => e.stopPropagation()}
      onchange={() => onToggleDir(node)}
    />
    <span class="sp-row" style="padding-left: {depth * 28}px">
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <span class="sp-arrow" onclick={(e) => { e.stopPropagation(); onToggleCollapse(node.path) }} title="展开/折叠">
        {isCollapsed(node.path) ? '▸' : '▾'}
      </span>
      <span class="sp-dir-icon">
        <svg viewBox="0 0 20 20" width="14" height="14" fill="currentColor" opacity="0.7"><path d="M2 5a2 2 0 012-2h4.586a1 1 0 01.707.293l1.414 1.414A1 1 0 0011.414 5H16a2 2 0 012 2v7a2 2 0 01-2 2H4a2 2 0 01-2-2V5z"/></svg>
      </span>
      <span class="sp-path sp-dir-name">{node.name}</span>
      {#if isCollapsed(node.path) && (ignored || dirAllIgnoredFn(node.path))}
        <span class="sp-ignored-badge">已忽略</span>
      {/if}
    </span>
  </div>
  {#if !isCollapsed(node.path)}
    {#each node.children as child (child.path)}
      <StatusTreeItem
        node={child}
        depth={depth + 1}
        {statusOf}
        {isChecked}
        {dirState}
        {onToggleFile}
        {onToggleDir}
        {isCollapsed}
        {onToggleCollapse}
        {onContextMenu}
        ignored={ignored || isIgnoredFn(child.path)}
        {isIgnoredFn}
        {dirAllIgnoredFn}
        {rollbackChangeSet}
        {rollbackDeletedSet}
        {rollbackModifiedSet}
        {previewMarked}
      />
    {/each}
  {/if}
{:else}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="sp-item {stClass}" class:sp-ignored={ignored} onclick={() => { if (!ignored && !rollbackDeleted) onToggleFile(node.path) }} oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); onContextMenu(node.path, false, e.clientX, e.clientY) }}>
    <input
      type="checkbox"
      class="sp-check"
      checked={isChecked(node.path)}
      disabled={ignored || rollbackDeleted}
      onclick={(e) => e.stopPropagation()}
      onchange={() => onToggleFile(node.path)}
      title={rollbackDeleted ? '已检出删除（该版本已删除此文件，记录时算删除）' : ignored ? '已忽略，不参与提交' : '取消勾选可单独提交其余文件'}
    />
    <span class="sp-row" style="padding-left: {depth * 28}px">
      <span class="sp-icon" style="color: {statusColor(status?.x || ' ', status?.y || ' ')}">
        {#if statusIconType(status?.x || ' ', status?.y || ' ') === 'added'}
          <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 2a.5.5 0 01.5.5v5h5a.5.5 0 010 1h-5v5a.5.5 0 01-1 0v-5h-5a.5.5 0 010-1h5v-5A.5.5 0 018 2z"/></svg>
        {:else if statusIconType(status?.x || ' ', status?.y || ' ') === 'deleted'}
          <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M4 8a.5.5 0 01.5-.5h7a.5.5 0 010 1h-7A.5.5 0 014 8z"/></svg>
        {:else if statusIconType(status?.x || ' ', status?.y || ' ') === 'modified'}
          <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M12.854.146a.5.5 0 00-.707 0L10.5 1.793 14.207 5.5l1.647-1.646a.5.5 0 000-.708l-3-3zm.646 6.061L9.793 2.5 3.293 9H3.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.5h.5a.5.5 0 01.5.5v.207l6.5-6.5zm-7.468 7.468A.5.5 0 016 13.5V13h-.5a.5.5 0 01-.5-.5V12h-.5a.5.5 0 01-.5-.5V11h-.5a.5.5 0 01-.5-.5V10h-.5a.499.499 0 01-.175-.032l-.179.178a.5.5 0 00-.11.168l-2 5a.5.5 0 00.65.65l5-2a.5.5 0 00.168-.11l.178-.178z"/></svg>
        {:else}
          <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 15A7 7 0 118 1a7 7 0 010 14zm0 1A8 8 0 108 0a8 8 0 000 16z"/><path d="M5.255 5.786a.237.237 0 00.241.247h.825c.138 0 .248-.113.266-.25.09-.656.54-1.134 1.342-1.134.686 0 1.314.343 1.314 1.168 0 .635-.374.927-.965 1.371-.673.489-1.206 1.06-1.168 1.987l.003.217a.25.25 0 00.25.246h.811a.25.25 0 00.25-.25v-.105c0-.718.273-.927 1.01-1.486.609-.463 1.244-.977 1.244-2.056 0-1.511-1.276-2.241-2.673-2.241-1.267 0-2.655.59-2.75 2.286zm1.557 5.763c0 .533.425.927 1.01.927.609 0 1.028-.394 1.028-.927 0-.552-.42-.94-1.029-.94-.584 0-1.009.388-1.009.94z"/></svg>
        {/if}
      </span>
      <span class="sp-ext-badge">{ext(node.path)}</span>
      <span class="sp-path">{node.name}</span>
      {#if ignored}
        <span class="sp-ignored-badge">已忽略</span>
      {/if}
      <span class="sp-badges">
        {#if previewMarked}
          <span class="sp-badge sp-badge-3d" title="记录时生成此文件的 3D 预览（右键取消）">3D</span>
        {/if}
        {#if rollbackDeleted}
          <span class="sp-badge sp-badge-rbd">已检出删除</span>
        {:else if rollbackMark}
          <span class="sp-badge sp-badge-rb">{rollbackModified ? '已修改' : '已检出'}</span>
        {:else if status?.x && status.x !== ' '}
          <span class="sp-badge" style="color: {statusColor(status.x, status.y)}">{statusLabel(status.x, status.y)}</span>
        {/if}
      </span>
    </span>
  </div>
{/if}

<style>
  .sp-row {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .sp-arrow {
    width: 12px;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    color: var(--text-dim);
  }
  .sp-dir-icon {
    width: 16px;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--text-dim);
  }
  .sp-dir-name {
    font-weight: 600;
  }
  :global(.sp-item.sp-ignored) {
    opacity: 0.5;
  }
  :global(.sp-item.sp-ignored .sp-path) {
    text-decoration: line-through;
    color: var(--text-dim);
  }
  :global(.sp-item.sp-ignored .sp-check) {
    opacity: 0.4;
  }
  .sp-ignored-badge {
    font-size: 9px;
    font-weight: 600;
    color: var(--text-dim);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 5px;
    flex-shrink: 0;
    letter-spacing: 0.5px;
  }
  :global(.sp-badge-3d) {
    color: var(--accent) !important;
    border-color: var(--accent) !important;
    font-weight: 700 !important;
  }
</style>
