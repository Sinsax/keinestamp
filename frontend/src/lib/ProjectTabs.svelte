<script lang="ts">
  import type { RecentEntry } from '../types'

  let {
    entries,
    activePath,
    onSelect,
    onNewProject,
    onSettings,
  }: {
    entries: RecentEntry[]
    activePath: string
    onSelect: (path: string) => void
    onNewProject: () => void
    onSettings: () => void
  } = $props()

  function cleanPath(p: string): string {
    return p.replace(/^\\\\\?\\/, '')
  }

  function projectName(p: string): string {
    return cleanPath(p).split(/[/\\]/).pop() || cleanPath(p)
  }
</script>

<div class="project-tabs">
  {#each entries as e}
    <button
      class="project-tab"
      class:active={e.path === activePath}
      onclick={() => onSelect(e.path)}
      title={cleanPath(e.path)}
    >
      <span class="tab-name">{projectName(e.path)}</span>
    </button>
  {/each}
  <button class="project-tab add-tab" onclick={onNewProject} title="New project">
    <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor">
      <path d="M8 2a.5.5 0 0 1 .5.5v5h5a.5.5 0 0 1 0 1h-5v5a.5.5 0 0 1-1 0v-5h-5a.5.5 0 0 1 0-1h5v-5A.5.5 0 0 1 8 2z"/>
    </svg>
  </button>
  <div class="tabs-spacer"></div>
  <button class="project-tab settings-tab" onclick={onSettings} title="Settings">
    <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
      <path d="M8 4.754a3.246 3.246 0 100 6.492 3.246 3.246 0 000-6.492zM5.754 8a2.246 2.246 0 114.492 0 2.246 2.246 0 01-4.492 0z"/>
      <path d="M9.796 1.343c-.527-1.79-3.065-1.79-3.592 0l-.094.319a.873.873 0 01-1.255.52l-.292-.16c-1.64-.892-3.433.902-2.54 2.541l.159.292a.873.873 0 01-.52 1.255l-.319.094c-1.79.527-1.79 3.065 0 3.592l.319.094a.873.873 0 01.52 1.255l-.16.292c-.892 1.64.901 3.434 2.541 2.54l.292-.159a.873.873 0 011.255.52l.094.319c.527 1.79 3.065 1.79 3.592 0l.094-.319a.873.873 0 011.255-.52l.292.16c1.64.893 3.434-.902 2.54-2.541l-.159-.292a.873.873 0 01.52-1.255l.319-.094c1.79-.527 1.79-3.065 0-3.592l-.319-.094a.873.873 0 01-.52-1.255l.16-.292c.893-1.64-.902-3.433-2.541-2.54l-.292.159a.873.873 0 01-1.255-.52l-.094-.319zm-2.633.283c.246-.835 1.428-.835 1.674 0l.094.319a1.873 1.873 0 002.693 1.115l.291-.16c.764-.415 1.6.42 1.184 1.185l-.159.292a1.873 1.873 0 001.116 2.692l.318.094c.835.246.835 1.428 0 1.674l-.319.094a1.873 1.873 0 00-1.115 2.693l.16.291c.415.764-.42 1.6-1.185 1.184l-.291-.159a1.873 1.873 0 00-2.693 1.116l-.094.318c-.246.835-1.428.835-1.674 0l-.094-.319a1.873 1.873 0 00-2.692-1.115l-.292.16c-.764.415-1.6-.42-1.184-1.185l.159-.291A1.873 1.873 0 001.945 8.93l-.319-.094c-.835-.246-.835-1.428 0-1.674l.319-.094A1.873 1.873 0 003.06 4.377l-.16-.292c-.415-.764.42-1.6 1.185-1.184l.292.159a1.873 1.873 0 002.692-1.115l.094-.319z"/>
    </svg>
  </button>
</div>

<style>
  .project-tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 8px;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
    min-height: 32px;
  }
  .project-tab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    font-size: 11px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 5px;
    cursor: pointer;
    color: var(--text-dim);
    white-space: nowrap;
    max-width: 160px;
    transition: background 0.1s, color 0.1s, border-color 0.1s;
  }
  .project-tab:hover {
    background: var(--surface2);
    color: var(--text);
    border-color: var(--surface3);
  }
  .project-tab.active {
    background: var(--surface2);
    color: var(--text);
    border-color: var(--accent);
    border-bottom-color: var(--surface2);
  }
  .tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .add-tab {
    padding: 3px 6px;
    background: transparent;
    border-color: transparent;
    color: var(--text-dim);
  }
  .add-tab:hover {
    background: var(--surface);
    border-color: var(--border);
    color: var(--text);
  }
  .tabs-spacer { flex: 1; }
  .settings-tab {
    padding: 3px 6px;
    background: transparent;
    border-color: transparent;
    color: var(--text-dim);
    margin-left: 2px;
  }
  .settings-tab:hover {
    background: var(--surface);
    border-color: var(--border);
    color: var(--text);
  }
</style>
