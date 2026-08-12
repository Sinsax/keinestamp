<script lang="ts">
  import { parseStatus, ruleHidden, findRuleHidden, type StatusFile } from './statusParse'
  import StatusTreeItem from './StatusTreeItem.svelte'
  import { buildFileTree, type FileTreeNode } from './fileTree'
  import { untrack } from 'svelte'
  import { openFolder, addToGitignore, getIgnoreInfo, addSkipWorktree, removeIgnore } from '../api'

  let {
    statusOutput,
    onRefresh,
    activePath = '',
    selection = null,
    statusError = false,
    statusErrorMsg = '',
    rollbackActive = false,
    trackedFiles = [],
    rollbackChangeFiles = [],
    rollbackDeletedFiles = [],
    rollbackModifiedFiles = [],
    onSelectionChange = (_sel: string[] | null) => {},
  }: {
    statusOutput: string
    onRefresh: () => void
    activePath?: string
    selection?: string[] | null
    statusError?: boolean
    statusErrorMsg?: string
    rollbackActive?: boolean
    trackedFiles?: string[]
    rollbackChangeFiles?: string[]
    rollbackDeletedFiles?: string[]
    rollbackModifiedFiles?: string[]
    onSelectionChange?: (sel: string[] | null) => void
  } = $props()

  const HIDDEN_PREFIXES = ['.gitignore', '.gitattributes', '.keinestamp']

  function isHiddenPath(p: string): boolean {
    return HIDDEN_PREFIXES.some((h) => p === h || p.startsWith(h + '/'))
  }

  const statusFiles = $derived(
    parseStatus(statusOutput).filter((f) => !isHiddenPath(f.path))
  )
  const statusMap = $derived(Object.fromEntries(statusFiles.map((f) => [f.path, f])) as Record<string, StatusFile>)
  // 回退版本改动标记：change_files（检出）+ deleted_files（回退删除，只读）+ modified_files（真修改）
  const rollbackChangeSet = $derived(new Set(rollbackActive ? rollbackChangeFiles : []))
  const rollbackDeletedSet = $derived(new Set(rollbackActive ? rollbackDeletedFiles : []))
  const rollbackModifiedSet = $derived(new Set(rollbackActive ? rollbackModifiedFiles : []))
  const visibleFiles = $derived.by(() => {
    const seen = new Set<string>()
    const out: string[] = []
    for (const f of statusFiles) {
      if (!seen.has(f.path)) {
        seen.add(f.path)
        out.push(f.path)
      }
    }
    // 被忽略文件（规则匹配）灰显加入树：匹配真实文件而非规则文本（如 scene.blend 而非 *.blend）
    for (const p of trackedFiles) {
      if (isHiddenPath(p)) continue
      if (seen.has(p)) continue
      if (isIgnored(p)) {
        seen.add(p)
        out.push(p)
      }
    }
    // 回退删除文件：磁盘保留、只读标记（回退状态下；marker 清除后自然不在列表）
    for (const p of rollbackDeletedFiles) {
      if (isHiddenPath(p)) continue
      if (seen.has(p)) continue
      seen.add(p)
      out.push(p)
    }
    // 回退检出文件：全部显示（含两版共有的——status 无变化不显示；含被忽略文件——灰色已检出）
    for (const p of rollbackChangeFiles) {
      if (isHiddenPath(p)) continue
      if (seen.has(p)) continue
      seen.add(p)
      out.push(p)
    }
    return out
  })
  const tree = $derived(buildFileTree(visibleFiles))
  const fileSet = $derived(new Set(statusFiles.map((f) => f.path)))
  const selectionSet = $derived(selection === null ? null : new Set(selection))
  const selectedCount = $derived(
    selection === null ? statusFiles.length : selection.filter((p) => fileSet.has(p)).length
  )
  const allSelected = $derived(selection === null || selectedCount === statusFiles.length)

  const dirFilesMap = $derived.by(() => {
    const map: Record<string, string[]> = {}
    const walk = (nodes: FileTreeNode[]) => {
      for (const n of nodes) {
        if (n.children.length === 0) continue
        const collected: string[] = []
        const collect = (nn: FileTreeNode) => {
          if (nn.children.length === 0) {
            if (!isIgnored(nn.path)) collected.push(nn.path)
          } else nn.children.forEach(collect)
        }
        collect(n)
        map[n.path] = collected
        walk(n.children)
      }
    }
    walk(tree)
    return map
  })

  const dirAllIgnoredMap = $derived.by(() => {
    const map: Record<string, boolean> = {}
    const walk = (nodes: FileTreeNode[]) => {
      for (const n of nodes) {
        if (n.children.length === 0) continue
        const leaves: string[] = []
        const collect = (nn: FileTreeNode) => {
          if (nn.children.length === 0) leaves.push(nn.path)
          else nn.children.forEach(collect)
        }
        collect(n)
        map[n.path] = leaves.length === 0 || leaves.every((f) => isIgnored(f))
        walk(n.children)
      }
    }
    walk(tree)
    return map
  })

  function dirAllIgnored(path: string): boolean {
    return dirAllIgnoredMap[path] ?? false
  }

  let collapsedDirs = $state<Set<string>>(new Set())
  let userExpanded = $state<Set<string>>(new Set())
  let userCollapsed = $state<Set<string>>(new Set())

  const autoFoldable = $derived.by(() => {
    const s = new Set<string>()
    for (const d of Object.keys(dirAllIgnoredMap)) {
      if ((dirAllIgnoredMap[d] || isIgnored(d)) && !userExpanded.has(d)) s.add(d)
    }
    return s
  })

  $effect(() => {
    const next = new Set(collapsedDirs)
    let changed = false
    for (const d of autoFoldable) {
      if (!next.has(d)) { next.add(d); changed = true }
    }
    for (const d of collapsedDirs) {
      if (!autoFoldable.has(d) && !userCollapsed.has(d)) { next.delete(d); changed = true }
    }
    if (changed) collapsedDirs = next
  })

  $effect(() => {
    activePath
    collapsedDirs = new Set()
    userExpanded = new Set()
    userCollapsed = new Set()
  })

  function isCollapsed(path: string): boolean {
    return collapsedDirs.has(path)
  }

  function toggleCollapse(path: string) {
    const next = new Set(collapsedDirs)
    const ue = new Set(userExpanded)
    const uc = new Set(userCollapsed)
    if (next.has(path)) {
      next.delete(path)
      ue.add(path)
      uc.delete(path)
    } else {
      next.add(path)
      uc.add(path)
      ue.delete(path)
    }
    collapsedDirs = next
    userExpanded = ue
    userCollapsed = uc
  }

  function isChecked(path: string): boolean {
    if (isIgnored(path)) return selectionSet !== null && selectionSet.has(path)
    return selectionSet === null || selectionSet.has(path)
  }

  function toggleFile(path: string) {
    const base = selection === null
      ? statusFiles.map((f) => f.path)
      : [...selection]
    const next = base.includes(path)
      ? base.filter((p) => p !== path)
      : [...base, path]
    const allSel =
      statusFiles.length === next.length &&
      statusFiles.every((f) => next.includes(f.path))
    onSelectionChange(allSel ? null : next)
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
    const base = selection === null ? statusFiles.map((f) => f.path) : [...selection]
    const baseSet = new Set(base)
    const allIn = fs.every((p) => baseSet.has(p))
    const next = allIn
      // 取消：移除目录下文件 + 目录本身（目录路径残留会导致记录时 stage 目录 = 全提）
      ? base.filter((p) => p !== node.path && !fs.includes(p))
      : Array.from(new Set([...base, ...fs, node.path]))
    const allSel =
      statusFiles.length === next.length &&
      statusFiles.every((f) => next.includes(f.path))
    onSelectionChange(allSel ? null : next)
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

  let contextMenu = $state<{
    cursorX: number
    cursorY: number
    x?: number
    y?: number
    path: string
    isDir: boolean
  } | null>(null)
  let trackedSet = $state<Set<string>>(new Set())
  let ignoredSet = $state<Set<string>>(new Set())
  let ruleIgnoredSet = $state<Set<string>>(new Set())
  let hiddenCollapsed = $state(true)
  let hiddenInput = $state('')

  // 预设规则（.keinestamp/ 下配置与缓存）不显示在管理列表，但保留在 .loreignore 中
  const PRESET_HIDDEN = new Set(['.keinestamp/.keinestamp.toml', '.keinestamp/web/timeline.json'])
  const userHiddenRules = $derived([...ignoredSet].filter((r) => !PRESET_HIDDEN.has(r)))
  let menuEl = $state<HTMLElement | null>(null)

  function openContextMenu(path: string, isDir: boolean, x: number, y: number) {
    contextMenu = { cursorX: x, cursorY: y, path, isDir }
  }

  $effect(() => {
    const cm = contextMenu
    const el = menuEl
    if (!cm || !el) return
    const w = el.offsetWidth
    const h = el.offsetHeight
    const margin = 8
    const vw = window.innerWidth
    const vh = window.innerHeight
    const px = Math.max(margin, Math.min(cm.cursorX, vw - w - margin))
    const py = Math.max(margin, Math.min(cm.cursorY, vh - h - margin))
    if (cm.x !== px || cm.y !== py) {
      cm.x = px
      cm.y = py
    }
  })

  $effect(() => {
    const path = activePath
    statusOutput
    if (!path) {
      trackedSet = new Set()
      ignoredSet = new Set()
      ruleIgnoredSet = new Set()
      return
    }
    // untrack：写 ruleIgnoredSet 不触发本 effect 重跑（避免循环刷屏）
    const files = untrack(() => statusFiles.map((f) => f.path))
    getIgnoreInfo(path, files)
      .then((info) => {
        if (path !== activePath) return
        trackedSet = new Set(info.tracked)
        ignoredSet = new Set(info.ignored)
        ruleIgnoredSet = new Set(info.rule_ignored)
      })
      .catch(() => {})
  })

  function isIgnored(path: string): boolean {
    if (ruleHidden(path, ignoredSet) || ruleHidden(path, ruleIgnoredSet)) return true
    const parts = path.split('/')
    for (let i = 1; i < parts.length; i++) {
      const prefix = parts.slice(0, i).join('/')
      if (ruleHidden(prefix, ignoredSet) || ruleHidden(prefix, ruleIgnoredSet)) return true
    }
    return false
  }

  function isTracked(path: string): boolean {
    if (trackedSet.has(path)) return true
    const parts = path.split('/')
    for (let i = 1; i < parts.length; i++) {
      if (trackedSet.has(parts.slice(0, i).join('/'))) return true
    }
    return false
  }

  async function refreshIgnoreInfo() {
    if (!activePath) return
    try {
      const files = statusFiles.map((f) => f.path)
      const info = await getIgnoreInfo(activePath, files)
      trackedSet = new Set(info.tracked)
      ignoredSet = new Set(info.ignored)
      ruleIgnoredSet = new Set(info.rule_ignored)
    } catch {}
  }

  async function addHiddenRule() {
    const rule = hiddenInput.trim()
    if (!rule || !activePath) return
    try {
      await addToGitignore(activePath, rule)
      hiddenInput = ''
      await refreshIgnoreInfo()
      onRefresh()
    } catch (e) {
      console.error('add hidden rule failed', e)
    }
  }

  async function removeHiddenRule(rule: string) {
    if (!activePath) return
    try {
      await removeIgnore(activePath, rule)
      await refreshIgnoreInfo()
      onRefresh()
    } catch (e) {
      console.error('remove hidden rule failed', e)
    }
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

  async function menuIgnore(kind: 'file' | 'ext' | 'dir') {
    const menu = contextMenu
    closeContextMenu()
    if (!menu) return
    let rule: string
    if (kind === 'dir') {
      rule = `${menu.path}/`
    } else if (kind === 'ext') {
      const name = menu.path.split('/').pop() || ''
      const dot = name.lastIndexOf('.')
      const ext = dot >= 0 ? name.slice(dot + 1) : ''
      rule = `*.${ext}`
    } else {
      rule = menu.path
    }
    try {
      if (kind === 'file' && isTracked(menu.path)) {
        await addSkipWorktree(activePath, menu.path)
      } else {
        await addToGitignore(activePath, rule)
      }
      await refreshIgnoreInfo()
      onRefresh()
    } catch (e) {
      console.error('add ignore failed', e)
    }
  }

  async function menuUnignore() {
    const menu = contextMenu
    closeContextMenu()
    if (!menu) return
    // 删除命中的规则（路径 → 规则文本：scene.blend → *.blend），而非传路径本身
    const rule =
      findRuleHidden(menu.path, ignoredSet) ||
      findRuleHidden(menu.path, ruleIgnoredSet) ||
      (menu.isDir ? `${menu.path}/` : menu.path)
    try {
      await removeIgnore(activePath, rule)
      await refreshIgnoreInfo()
      onRefresh()
    } catch (e) {
      console.error('remove ignore failed', e)
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="status-panel">
  <div class="sp-header">
    <span class="sp-title">变更</span>
    <span class="sp-count">{selectedCount}/{statusFiles.length} 个文件</span>
    <div class="sp-spacer"></div>
    {#if !allSelected}
      <button class="sp-select-all" onclick={selectAll} title="全部选中">全选</button>
    {/if}
    {#if selectedCount > 0}
      <button class="sp-select-all" onclick={selectNone} title="全部取消">全不选</button>
    {/if}
    <button class="sp-refresh" onclick={onRefresh} title="刷新状态">
      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 3a5 5 0 014.546 2.916.5.5 0 01-.908.417A4 4 0 108 11.5a4.002 4.002 0 003.392-1.87.5.5 0 11.854.52A5 5 0 113 8a5 5 0 015-5z"/><path d="M11.5 3.5A.5.5 0 0112 4v2.5a.5.5 0 01-.5.5H9a.5.5 0 010-1h1.802A4.003 4.003 0 004 8a.5.5 0 01-1 0 5 5 0 015-5 .5.5 0 01.5.5z"/></svg>
    </button>
  </div>

  {#if statusFiles.length === 0 && ignoredSet.size === 0}
    <div class="sp-empty">
      <div class="sp-empty-icon"><svg viewBox="0 0 16 16" width="28" height="28" fill="currentColor" opacity="0.5"><path d="M13.854 3.646a.5.5 0 010 .708l-7 7a.5.5 0 01-.708 0l-3.5-3.5a.5.5 0 11.708-.708L6.5 10.293l6.646-6.647a.5.5 0 01.708 0z"/></svg></div>
      {#if statusError}
        <div>状态获取失败</div>
        <div style="font-size:12px" title={statusErrorMsg}>{statusErrorMsg || 'Lore 状态读取出错，点击刷新重试'}</div>
      {:else}
        <div>无变更 — 工作区干净</div>
      {/if}
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
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
          ignored={isIgnored(node.path)}
          isIgnoredFn={isIgnored}
          dirAllIgnoredFn={dirAllIgnored}
          rollbackChangeSet={rollbackChangeSet}
          rollbackDeletedSet={rollbackDeletedSet}
          rollbackModifiedSet={rollbackModifiedSet}
        />
      {/each}
    </div>
  {/if}

  <!-- 忽略文件/格式管理（默认折叠；规则存项目级 .loreignore，随仓库历史；手动输入规则） -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="sp-hidden-block">
    <div
      class="sp-hidden-header"
      onclick={() => (hiddenCollapsed = !hiddenCollapsed)}
      title="忽略文件/格式：手动输入规则（如 cache/、*.tmp），或在变更列表右键设置"
    >
      <span class="sp-hidden-caret">{hiddenCollapsed ? '▸' : '▾'}</span>
      <span>忽略文件/格式</span>
      {#if userHiddenRules.length > 0}
        <span class="sp-hidden-count">{userHiddenRules.length}</span>
      {/if}
    </div>
    {#if !hiddenCollapsed}
      <div class="sp-hidden-body">
        <span class="sp-hidden-hint">手动输入规则（如 cache/、*.tmp），或右键变更列表中的文件设置</span>
        <div class="sp-hidden-list">
          {#each userHiddenRules as rule}
            <div class="sp-hidden-row">
              <span class="sp-hidden-rule" title={rule}>{rule}</span>
              <button class="sp-hidden-del" title="移除隐藏" onclick={() => removeHiddenRule(rule)}>×</button>
            </div>
          {/each}
          {#if userHiddenRules.length === 0}
            <span class="sp-hidden-empty">无用户隐藏规则</span>
          {/if}
        </div>
        <div class="sp-hidden-input-row">
          <input
            type="text"
            bind:value={hiddenInput}
            placeholder="输入路径/格式，如 cache/ 或 *.tmp"
            onkeydown={(e) => { if (e.key === 'Enter') addHiddenRule() }}
          />
          <button onclick={addHiddenRule} disabled={!hiddenInput.trim()}>添加</button>
        </div>
      </div>
    {/if}
  </div>

  {#if contextMenu}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="ctx-backdrop" onclick={closeContextMenu} oncontextmenu={(e) => { e.preventDefault(); closeContextMenu() }}></div>
    <div class="ctx-menu" bind:this={menuEl} style="left: {contextMenu.x ?? contextMenu.cursorX}px; top: {contextMenu.y ?? contextMenu.cursorY}px">
      <button onclick={menuOpenFolder}>打开所在文件夹</button>
      <button onclick={menuCopyPath}>复制路径</button>
      <button onclick={menuCopyName}>复制名称</button>
      <div class="ctx-sep"></div>
      {#if isIgnored(contextMenu.path)}
        <button onclick={menuUnignore}>取消忽略</button>
      {:else if contextMenu.isDir}
        <button onclick={() => menuIgnore('dir')}>忽略目录</button>
      {:else if isTracked(contextMenu.path)}
        <button onclick={() => menuIgnore('file')} title="本地忽略，可随时恢复">忽略文件</button>
      {:else}
        <button onclick={() => menuIgnore('file')}>忽略文件</button>
        <button onclick={() => menuIgnore('ext')}>忽略格式</button>
      {/if}
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
  /* 整行状态着色：回退改动/已修改=黄、新增=绿、回退删除/已删除=红、未跟踪=灰（hover 加深覆盖默认） */
  :global(.sp-item.sp-st-m) { background: color-mix(in srgb, var(--warning) 16%, transparent); }
  :global(.sp-item.sp-st-m:hover) { background: color-mix(in srgb, var(--warning) 26%, transparent); }
  :global(.sp-item.sp-st-a) { background: color-mix(in srgb, var(--success) 14%, transparent); }
  :global(.sp-item.sp-st-a:hover) { background: color-mix(in srgb, var(--success) 24%, transparent); }
  :global(.sp-item.sp-st-d) { background: color-mix(in srgb, var(--error) 12%, transparent); }
  :global(.sp-item.sp-st-d:hover) { background: color-mix(in srgb, var(--error) 22%, transparent); }
  :global(.sp-item.sp-st-u) { background: color-mix(in srgb, var(--text-dim) 10%, transparent); }
  :global(.sp-item.sp-st-u:hover) { background: color-mix(in srgb, var(--text-dim) 18%, transparent); }
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
  :global(.sp-badges) {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  :global(.sp-badge) {
    font-size: 10px;
    font-weight: 500;
    text-transform: uppercase;
    padding: 0 5px;
    border-radius: 4px;
    white-space: nowrap;
  }
  :global(.sp-badge.sp-badge-rb) {
    background: color-mix(in srgb, var(--warning) 22%, transparent);
    color: var(--warning);
  }
  :global(.sp-badge.sp-badge-rbd) {
    background: color-mix(in srgb, var(--error) 16%, transparent);
    color: var(--error);
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
  .ctx-menu button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
    pointer-events: none;
  }
  .ctx-sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
  .sp-hidden-block {
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .sp-hidden-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
    cursor: pointer;
    user-select: none;
  }
  .sp-hidden-header:hover {
    color: var(--accent);
  }
  .sp-hidden-caret {
    font-size: 9px;
  }
  .sp-hidden-count {
    font-size: 10px;
    padding: 0 6px;
    border-radius: 8px;
    background: var(--surface2);
    color: var(--text-dim);
  }
  .sp-hidden-body {
    padding: 0 10px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .sp-hidden-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 140px;
    overflow-y: auto;
  }
  .sp-hidden-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 3px 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 5px;
    font-size: 11px;
  }
  .sp-hidden-rule {
    font-family: var(--mono, monospace);
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sp-hidden-del {
    border: none;
    background: none;
    color: var(--text-dim);
    font-size: 13px;
    cursor: pointer;
    padding: 0 3px;
    line-height: 1;
  }
  .sp-hidden-del:hover {
    color: var(--error);
  }
  .sp-hidden-empty {
    font-size: 11px;
    color: var(--text-dim);
    padding: 2px 0;
  }
  .sp-hidden-hint {
    font-size: 10px;
    color: var(--text-dim);
  }
  .sp-hidden-input-row {
    display: flex;
    gap: 6px;
  }
  .sp-hidden-input-row input {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    padding: 3px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
  }
  .sp-hidden-input-row button {
    font-size: 11px;
    padding: 3px 10px;
    border-radius: 5px;
    cursor: pointer;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    white-space: nowrap;
  }
  .sp-hidden-input-row button:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .sp-hidden-input-row button:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
