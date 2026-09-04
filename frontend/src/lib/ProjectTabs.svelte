<script lang="ts">
  import type { RecentEntry } from '../types'


  let {
    entries,
    activePath,
    onSelect,
    onNewProject,
    onSettings,
    theme = 'day',
    onToggleTheme = () => {},
    onRemoveRecent = (_p: string) => {},
    onReorder = (_paths: string[]) => {},
  }: {
    entries: RecentEntry[]
    activePath: string
    onSelect: (path: string) => void
    onNewProject: () => void
    onSettings: () => void
    theme?: 'day' | 'night'
    onToggleTheme?: () => void
    onRemoveRecent?: (path: string) => void
    onReorder?: (paths: string[]) => void
  } = $props()

  let dragOrder = $state<string[] | null>(null)
  let draggedPath = $state<string | null>(null)
  let dragging = $state(false)
  let ghostPos = $state<{ x: number; y: number } | null>(null)
  let suppressClick = $state(false)
  let dragStartX = 0
  let dragStartY = 0

  const displayEntries = $derived.by(() => {
    const byPath = new Map(entries.map((e) => [e.path, e]))
    const order = dragOrder ?? entries.map((x) => x.path)
    return order
      .map((p) => byPath.get(p))
      .filter((e): e is RecentEntry => e !== undefined)
  })

  function handlePointerDown(e: PointerEvent, path: string) {
    if (e.button !== 0) return
    dragOrder = entries.map((x) => x.path)
    draggedPath = path
    dragStartX = e.clientX
    dragStartY = e.clientY
    dragging = false
    ghostPos = null
    window.addEventListener('pointermove', handlePointerMove)
    window.addEventListener('pointerup', handlePointerUp)
  }

  function handlePointerMove(e: PointerEvent) {
    if (!dragOrder) return
    if (!dragging) {
      if (Math.hypot(e.clientX - dragStartX, e.clientY - dragStartY) < 5) return
      dragging = true
    }
    ghostPos = { x: e.clientX + 10, y: e.clientY + 14 }
    const el = document
      .elementFromPoint(e.clientX, e.clientY)
      ?.closest<HTMLElement>('.project-tab[data-idx]')
    if (!el) return
    const targetIdx = Number(el.dataset.idx)
    const targetPath = displayEntries[targetIdx]?.path
    if (!targetPath || targetPath === draggedPath) return
    const from = dragOrder.indexOf(draggedPath ?? '')
    const to = dragOrder.indexOf(targetPath)
    if (from === -1 || to === -1 || from === to) return
    const next = [...dragOrder]
    const [moved] = next.splice(from, 1)
    next.splice(to, 0, moved)
    dragOrder = next
  }

  function handlePointerUp() {
    window.removeEventListener('pointermove', handlePointerMove)
    window.removeEventListener('pointerup', handlePointerUp)
    if (dragOrder && dragging) {
      const changed =
        dragOrder.length === entries.length &&
        dragOrder.some((p, i) => entries[i]?.path !== p)
      if (changed) onReorder(dragOrder)
    }
    if (dragging) suppressClick = true
    dragOrder = null
    draggedPath = null
    dragging = false
    ghostPos = null
  }

  function handleTabClick(e: MouseEvent, path: string) {
    if (suppressClick) {
      suppressClick = false
      return
    }
    if (path === activePath) return
    onSelect(path)
  }

  function cleanPath(p: string): string {
    return p.replace(/^\\\\\?\\/, '')
  }

  function projectName(p: string): string {
    return cleanPath(p).split(/[/\\]/).pop() || cleanPath(p)
  }

  let scrollEl = $state<HTMLElement | null>(null)

  function wheelScroll(node: HTMLElement) {
    const onWheel = (e: WheelEvent) => {
      if (Math.abs(e.deltaX) > Math.abs(e.deltaY)) return
      if (node.scrollWidth <= node.clientWidth) return
      const atEdge =
        (e.deltaY < 0 && node.scrollLeft <= 0) ||
        (e.deltaY > 0 && node.scrollLeft >= node.scrollWidth - node.clientWidth)
      if (atEdge) return
      e.preventDefault()
      node.scrollLeft += e.deltaY
    }
    node.addEventListener('wheel', onWheel, { passive: false })
    return {
      destroy() {
        node.removeEventListener('wheel', onWheel)
      },
    }
  }

  function handleTabClose(e: MouseEvent, path: string) {
    e.stopPropagation()
    onRemoveRecent(path)
  }

  let isScrollable = $state(false)

  $effect(() => {
    const el = scrollEl
    if (!el) return
    const check = () => { isScrollable = el.scrollWidth > el.clientWidth }
    check()
    const ro = new ResizeObserver(check)
    ro.observe(el)
    entries
    return () => ro.disconnect()
  })

  $effect(() => {
    activePath
    if (!scrollEl) return
    const active = scrollEl.querySelector<HTMLElement>('.project-tab.active')
    active?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  })
</script>

<div class="project-tabs">
  <div class="tabs-scroll" class:scrollable={isScrollable} use:wheelScroll bind:this={scrollEl}>
    {#each displayEntries as e, i (e.path)}
        <button
          class="project-tab"
          class:active={e.path === activePath}
          class:dragging={dragging && draggedPath === e.path}
          data-idx={i}
          onclick={(ev) => handleTabClick(ev, e.path)}
          onauxclick={(ev) => { if (ev.button === 1) handleTabClose(ev, e.path) }}
          onpointerdown={(ev) => handlePointerDown(ev, e.path)}
          title={cleanPath(e.path)}
        >
          <span class="tab-name">{projectName(e.path)}</span>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <span class="tab-close" onclick={(ev) => handleTabClose(ev, e.path)} title="移除仓库">
            <svg viewBox="0 0 12 12" width="9" height="9" fill="currentColor"><path d="M2.646 2.646a.5.5 0 0 1 .708 0L6 5.293l2.646-2.647a.5.5 0 0 1 .708.708L6.707 6l2.647 2.646a.5.5 0 0 1-.708.708L6 6.707l-2.646 2.647a.5.5 0 0 1-.708-.708L5.293 6 2.646 3.354a.5.5 0 0 1 0-.708z"/></svg>
          </span>
        </button>
    {/each}
    <button class="project-tab add-tab" onclick={onNewProject} title="New project">
      <svg viewBox="0 0 16 16" width="13" height="13" fill="currentColor">
        <path d="M8 2a.5.5 0 0 1 .5.5v5h5a.5.5 0 0 1 0 1h-5v5a.5.5 0 0 1-1 0v-5h-5a.5.5 0 0 1 0-1h5v-5A.5.5 0 0 1 8 2z"/>
      </svg>
    </button>
  </div>
  <div class="tabs-actions">
    <button class="project-tab theme-tab" onclick={onToggleTheme} title={theme === 'night' ? '切换到日间模式' : '切换到夜间模式'}>
      {#if theme === 'night'}
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 9.6A6.5 6.5 0 1 1 6.4 1.5a5.2 5.2 0 0 0 8.1 8.1z"/></svg>
      {:else}
        <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="8" cy="8" r="3.4"/><path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1"/></svg>
      {/if}
    </button>
    <button class="project-tab settings-tab" onclick={onSettings} title="Settings">
      <svg viewBox="0 0 16 16" width="13" height="13" fill="currentColor">
        <path d="M8 4.754a3.246 3.246 0 100 6.492 3.246 3.246 0 000-6.492zM5.754 8a2.246 2.246 0 114.492 0 2.246 2.246 0 01-4.492 0z"/>
        <path d="M9.796 1.343c-.527-1.79-3.065-1.79-3.592 0l-.094.319a.873.873 0 01-1.255.52l-.292-.16c-1.64-.892-3.433.902-2.54 2.541l.159.292a.873.873 0 01-.52 1.255l-.319.094c-1.79.527-1.79 3.065 0 3.592l.319.094a.873.873 0 01.52 1.255l-.16.292c-.892 1.64.901 3.434 2.541 2.54l.292-.159a.873.873 0 011.255.52l.094.319c.527 1.79 3.065 1.79 3.592 0l.094-.319a.873.873 0 011.255-.52l.292.16c1.64.893 3.434-.902 2.54-2.541l-.159-.292a.873.873 0 01.52-1.255l.319-.094c1.79-.527 1.79-3.065 0-3.592l-.319-.094a.873.873 0 01-.52-1.255l.16-.292c.893-1.64-.902-3.433-2.541-2.54l-.292.159a.873.873 0 01-1.255-.52l-.094-.319zm-2.633.283c.246-.835 1.428-.835 1.674 0l.094.319a1.873 1.873 0 002.693 1.115l.291-.16c.764-.415 1.6.42 1.184 1.185l-.159.292a1.873 1.873 0 001.116 2.692l.318.094c.835.246.835 1.428 0 1.674l-.319.094a1.873 1.873 0 00-1.115 2.693l.16.291c.415.764-.42 1.6-1.185 1.184l-.291-.159a1.873 1.873 0 00-2.693 1.116l-.094.318c-.246.835-1.428.835-1.674 0l-.094-.319a1.873 1.873 0 00-2.692-1.115l-.292.16c-.764.415-1.6-.42-1.184-1.185l.159-.291A1.873 1.873 0 001.945 8.93l-.319-.094c-.835-.246-.835-1.428 0-1.674l.319-.094A1.873 1.873 0 003.06 4.377l-.16-.292c-.415-.764.42-1.6 1.185-1.184l.292.159a1.873 1.873 0 002.692-1.115l.094-.319z"/>
      </svg>
    </button>
  </div>
</div>

{#if ghostPos && draggedPath}
  <div class="drag-ghost" style="left: {ghostPos.x}px; top: {ghostPos.y}px">{projectName(draggedPath)}</div>
{/if}

<style>
  .project-tabs {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 14px 6px;
    background: transparent;
    min-height: 36px;
    flex-shrink: 0;
  }
  .tabs-scroll {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 5px;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
    scrollbar-color: transparent transparent;
    padding: 3px 0;
  }
  .project-tabs:hover .tabs-scroll {
    scrollbar-color: var(--surface3) transparent;
  }
  .tabs-scroll::-webkit-scrollbar { height: 4px; }
  .tabs-scroll::-webkit-scrollbar-track { background: transparent; }
  .tabs-scroll::-webkit-scrollbar-thumb {
    background: transparent;
    border-radius: 2px;
  }
  .project-tabs:hover .tabs-scroll::-webkit-scrollbar-thumb {
    background: var(--surface3);
  }
  .tabs-scroll.scrollable {
    mask-image: linear-gradient(to right, transparent 0, #000 10px, #000 100%);
    -webkit-mask-image: linear-gradient(to right, transparent 0, #000 10px, #000 100%);
  }
  .tabs-actions {
    display: flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
    margin-left: 4px;
    padding-left: 8px;
    border-left: 1px solid var(--border);
  }
  .project-tab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    width: 136px;
    padding: 4px 12px;
    font-size: 11px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 999px;
    position: relative;
    cursor: pointer;
    color: var(--text-dim);
    white-space: nowrap;
    transition: background 0.12s var(--ease), color 0.12s var(--ease), border-color 0.12s var(--ease), box-shadow 0.12s var(--ease);
  }
  .project-tab:hover {
    background: color-mix(in srgb, var(--text-dim) 9%, transparent);
    color: var(--text);
  }
  .project-tab.dragging {
    opacity: 0.4;
  }
  .drag-ghost {
    position: fixed;
    z-index: 200;
    background: var(--surface);
    border: 1px solid var(--accent);
    border-radius: 999px;
    padding: 4px 12px;
    font-size: 11px;
    color: var(--accent);
    box-shadow: var(--shadow-md);
    pointer-events: none;
    white-space: nowrap;
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .project-tab.active {
    background: var(--surface);
    border-color: var(--border);
    box-shadow: var(--shadow-sm);
    color: var(--text);
    font-weight: 600;
  }
  .tab-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
  .tab-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    color: var(--text-dim);
    flex-shrink: 0;
    visibility: hidden;
    transition: background 0.12s, color 0.12s, visibility 0s;
  }
  .project-tab:hover .tab-close,
  .project-tab.active .tab-close {
    visibility: visible;
  }
  .tab-close:hover {
    background: var(--surface3);
    color: var(--text);
  }
  .add-tab {
    position: sticky;
    right: 0;
    margin-left: auto;
    width: auto;
    padding: 4px 9px;
    background: transparent;
    border: 1px dashed var(--border-strong);
    color: var(--text-dim);
    flex-shrink: 0;
    align-self: center;
    border-radius: 999px;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .add-tab:hover {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .settings-tab, .theme-tab {
    width: auto;
    padding: 4px 8px;
    background: transparent;
    border-color: transparent;
    color: var(--text-dim);
    flex-shrink: 0;
    align-self: center;
    border-radius: 999px;
  }
  .settings-tab:hover, .theme-tab:hover {
    background: color-mix(in srgb, var(--text-dim) 9%, transparent);
    color: var(--text);
  }
</style>
