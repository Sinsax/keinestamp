<script lang="ts">
  import DirTree from './DirTree.svelte'

  let {
    trackedFiles = [],
    activePath = '',
    collapsed = false,
    onExpand = () => {},
    onCloseProject = (_p: string) => {},
  }: {
    trackedFiles?: string[]
    activePath?: string
    collapsed?: boolean
    onExpand?: () => void
    onCloseProject: (path: string) => void
  } = $props()

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
        <DirTree paths={trackedFiles} {activePath} />
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

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .sidebar-count {
    margin-left: auto;
    font-size: 10px;
    background: var(--surface2);
    padding: 0 6px;
    border-radius: 8px;
    color: var(--text-dim);
    font-weight: 500;
    text-transform: none;
    letter-spacing: normal;
  }
  .sidebar-list {
    flex: 1;
    overflow-y: auto;
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
    padding: 6px 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 5px;
    color: var(--text-dim);
    font-size: 12px;
    cursor: pointer;
    transition: background 0.1s, color 0.1s, border-color 0.1s;
  }
  .close-btn:hover {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
  }
  .sidebar-collapsed-bar {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding-top: 10px;
  }
  .expand-btn {
    background: none;
    border: none;
    color: var(--text-dim);
    padding: 6px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .expand-btn:hover {
    background: var(--accent-soft);
    color: var(--accent);
  }
</style>
