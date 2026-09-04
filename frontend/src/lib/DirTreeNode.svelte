<script lang="ts">
  import DirTreeNode from './DirTreeNode.svelte'
  import type { FileTreeNode } from './fileTree'
  import { openFolder } from '../api'
  import FileTypeIcon from './FileTypeIcon.svelte'

  let {
    node,
    collapsed,
    toggle,
    depth = 0,
    activePath = '',
    onContextMenu = (_path: string, _isDir: boolean, _x: number, _y: number) => {},
  }: {
    node: FileTreeNode
    collapsed: Set<string>
    toggle: (path: string) => void
    depth?: number
    activePath?: string
    onContextMenu?: (path: string, isDir: boolean, x: number, y: number) => void
  } = $props()

  function handleFileDblClick() {
    if (isDir) return
    const relDir = node.path.includes('/') ? node.path.substring(0, node.path.lastIndexOf('/')) : ''
    const fullDir = relDir ? `${activePath}/${relDir}` : activePath
    openFolder(fullDir)
  }

  const isDir = $derived(node.isDir || node.children.length > 0)
  const isCollapsed = $derived(collapsed.has(node.path))
  const pad = $derived(depth * 18)
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="dtn"
  class:dir={isDir}
  style="padding-left: {pad}px"
  onclick={() => isDir && toggle(node.path)}
  oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); onContextMenu(node.path, isDir, e.clientX, e.clientY) }}
>
  {#if isDir}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <span class="arr" onclick={(e) => { e.stopPropagation(); toggle(node.path) }}>
      {#if isCollapsed}
        <svg viewBox="0 0 16 16" width="10" height="10" fill="currentColor"><path d="M6 3l5 5-5 5V3z"/></svg>
      {:else}
        <svg viewBox="0 0 16 16" width="10" height="10" fill="currentColor"><path d="M3 6l5 5 5-5H3z"/></svg>
      {/if}
    </span>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <span class="ico" onclick={(e) => { e.stopPropagation(); toggle(node.path) }}>
      {#if isCollapsed}
        <svg viewBox="0 0 20 20" width="16" height="16" fill="currentColor"><path d="M2 5a2 2 0 012-2h4.586a1 1 0 01.707.293l1.414 1.414A1 1 0 0011.414 5H16a2 2 0 012 2v7a2 2 0 01-2 2H4a2 2 0 01-2-2V5z" opacity="0.7"/></svg>
      {:else}
        <svg viewBox="0 0 20 20" width="16" height="16" fill="currentColor"><path d="M2 5a2 2 0 012-2h4.586a1 1 0 01.707.293l1.414 1.414A1 1 0 0011.414 5H16a2 2 0 012 2v7a2 2 0 01-2 2H4a2 2 0 01-2-2V5z"/></svg>
      {/if}
    </span>
  {:else}
    <span class="arr-placeholder"></span>
    <span class="ico"><FileTypeIcon path={node.path} size={14} /></span>
  {/if}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <span class="nm" class:dir={isDir} onclick={(e) => { if (isDir) { e.stopPropagation(); toggle(node.path) } }} ondblclick={!isDir ? handleFileDblClick : undefined}>{node.name}</span>
</div>

{#if isDir && !isCollapsed}
  {#each node.children as child (child.path)}
      <DirTreeNode node={child} {collapsed} {toggle} depth={depth + 1} {activePath} {onContextMenu} />
  {/each}
{/if}

<style>
  .dtn {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 2px 6px 2px 2px;
    margin: 0 4px;
    white-space: nowrap;
    border-radius: 4px;
    transition: background 0.1s var(--ease);
  }
  .dtn:hover { background: var(--surface2); }
  .arr {
    width: 14px; flex-shrink: 0;
    cursor: pointer;
    color: var(--text-dim);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .arr-placeholder {
    width: 14px; flex-shrink: 0;
    display: inline-block;
  }
  .ico {
    width: 20px; flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .nm {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .nm.dir { cursor: pointer; font-weight: 500; }
</style>
