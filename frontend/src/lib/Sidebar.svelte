<script lang="ts">
  import DirTree from './DirTree.svelte'
  import { openFolder } from '../api'

  let {
    trackedFiles = [],
    emptyDirs = [],
    activePath = '',
    collapsed = false,
    onExpand = () => {},
    onCloseProject = (_p: string) => {},
  }: {
    trackedFiles?: string[]
    emptyDirs?: string[]
    activePath?: string
    collapsed?: boolean
    onExpand?: () => void
    onCloseProject: (path: string) => void
  } = $props()

  let contextMenu = $state<{ x: number; y: number; path: string; isDir: boolean } | null>(null)

  function openContextMenu(path: string, isDir: boolean, x: number, y: number) {
    const MENU_W = 160
    const MENU_H = 140
    const px = Math.max(8, Math.min(x, window.innerWidth - MENU_W - 8))
    const py = Math.max(8, Math.min(y, window.innerHeight - MENU_H - 8))
    contextMenu = { x: px, y: py, path, isDir }
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
    const full = activePath ? `${activePath}/${dir}` : ''
    try {
      await openFolder(full)
    } catch {
      /* ignore */
    }
  }

  async function menuCopyPath() {
    const menu = contextMenu
    closeContextMenu()
    if (!menu) return
    const full = activePath ? `${activePath}/${menu.path}` : menu.path
    try {
      await navigator.clipboard.writeText(full)
    } catch { /* ignore */ }
  }

  async function menuCopyName() {
    const menu = contextMenu
    closeContextMenu()
    if (!menu) return
    const name = menu.path.split('/').pop() || menu.path
    try {
      await navigator.clipboard.writeText(name)
    } catch { /* ignore */ }
  }

  function cleanPath(p: string): string {
    return p.replace(/^\\\\\?\\/, '')
  }

  function projectName(p: string): string {
    return cleanPath(p).split(/[/\\]/).pop() || cleanPath(p)
  }
</script>

<div class="sidebar" class:collapsed>
  {#if collapsed}
    <div class="sidebar-collapsed-bar">
      <button class="expand-btn" onclick={onExpand} title="展开侧栏">
        <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M6 3l5 5-5 5V3z"/></svg>
      </button>
    </div>
  {:else}
    <div class="sidebar-header">
      {#if activePath}
        <span>{projectName(activePath)}</span>
        {#if trackedFiles.length > 0}
          <span class="sidebar-count">{trackedFiles.length}</span>
        {/if}
      {:else}
        <span>Files</span>
      {/if}
    </div>
    <div class="sidebar-list">
      {#if !activePath}
        <div class="sidebar-empty">未选择项目</div>
      {:else if trackedFiles.length === 0}
        <div class="sidebar-empty">
          <div class="sidebar-empty-icon"><svg viewBox="0 0 20 20" width="24" height="24" fill="currentColor" opacity="0.4"><path d="M2 4.5A1.5 1.5 0 013.5 3h5.757a1.5 1.5 0 011.06.44l1.122 1.12A1.5 1.5 0 0112 4.62V6h2.5A1.5 1.5 0 0116 7.5v6a1.5 1.5 0 01-1.5 1.5h-11A1.5 1.5 0 012 13.5v-9z"/></svg></div>
          <div>无文件</div>
        </div>
      {:else}
        <DirTree paths={[...trackedFiles, ...emptyDirs]} dirs={emptyDirs} {activePath} onContextMenu={openContextMenu} />
      {/if}
    </div>
    {#if activePath}
      <div class="sidebar-bottom">
        <button class="close-btn" onclick={() => onCloseProject(activePath)}>
          <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
            <path d="M2 2.5A1.5 1.5 0 0 1 3.5 1h7.828a1.5 1.5 0 0 1 1.06.44l1.122 1.12A1.5 1.5 0 0 1 14 3.62V13.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 13.5V2.5z"/>
          </svg>
          <span>退出文件夹</span>
        </button>
      </div>
    {/if}
  {/if}
</div>

{#if contextMenu}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="ctx-backdrop" onclick={closeContextMenu} oncontextmenu={(e) => { e.preventDefault(); closeContextMenu() }}></div>
  <div class="ctx-menu" style="left: {contextMenu.x}px; top: {contextMenu.y}px">
    <button onclick={menuOpenFolder}>打开所在文件夹</button>
    <button onclick={menuCopyPath}>复制路径</button>
    <button onclick={menuCopyName}>复制名称</button>
  </div>
{/if}

<svelte:window onkeydown={handleKeydown} />

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .sidebar-count {
    margin-left: auto;
    font-size: 9.5px;
    background: var(--surface2);
    padding: 0 6px;
    border-radius: 8px;
    color: var(--text-dim);
    font-weight: 600;
    text-transform: none;
    letter-spacing: normal;
    line-height: 1.5;
  }
  .sidebar-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }
  .sidebar-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 6px;
    color: var(--text-dim);
    font-size: 12px;
    padding: 20px;
    text-align: center;
  }
  .sidebar-empty-icon {
    font-size: 24px;
    opacity: 0.4;
  }
  .sidebar-bottom {
    border-top: 1px solid var(--border);
    padding: 6px 8px;
    flex-shrink: 0;
  }
  .close-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 9px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-dim);
    font-size: 11.5px;
    cursor: pointer;
    transition: background 0.1s, color 0.1s, border-color 0.1s;
  }
  .close-btn:hover {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .sidebar-collapsed-bar {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding-top: 8px;
  }
  .expand-btn {
    background: none;
    border: none;
    color: var(--text-dim);
    padding: 5px;
    border-radius: var(--radius-sm);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .expand-btn:hover {
    background: var(--surface2);
    color: var(--accent);
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
    max-height: calc(100vh - 16px);
    overflow-y: auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-lg);
    padding: 4px;
    font-size: 12px;
    animation: ctx-in 0.1s var(--ease);
  }
  @keyframes ctx-in {
    from { opacity: 0; transform: scale(0.98) translateY(-2px); }
    to { opacity: 1; transform: scale(1) translateY(0); }
  }
  .ctx-menu button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 5px 9px;
    background: none;
    border: none;
    border-radius: 4px;
    color: var(--text);
    cursor: pointer;
    font-size: 12px;
    transition: background 0.1s var(--ease), color 0.1s var(--ease);
  }
  .ctx-menu button:hover {
    background: var(--surface2);
    color: var(--accent);
  }
</style>
