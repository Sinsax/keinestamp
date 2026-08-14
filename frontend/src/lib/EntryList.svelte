<script lang="ts">
  import DirTree from './DirTree.svelte'
  import type { TimelineEntry } from '../types'
  import { setEntryMilestone, setEntryPreview, updateConfig } from '../api'
  import { tagColor } from './tags'

  let {
    entries,
    selectedId,
    activePath = '',
    onSelect,
    accessToken = '',
    onTokenChange,
    siteName = '',
    siteLocked = false,
    onSiteNameChange = async (_val: string) => false,
    onOpenSite = () => {},
    onOpenLocalView = () => {},
    onMilestoneChange = async () => {},
    onPreviewChange = async () => {},
    rollbackPoint = null,
    onExportEntry = (entry: TimelineEntry) => {},
    onRollback = (entry: TimelineEntry) => {},
    rollbackBusy = false,
  }: {
    entries: TimelineEntry[]
    selectedId: string
    activePath?: string
    onSelect: (id: string) => void
    accessToken?: string
    onTokenChange?: (val: string) => void
    siteName?: string
    siteLocked?: boolean
    onSiteNameChange?: (val: string) => Promise<boolean>
    onOpenSite?: () => void
    onOpenLocalView?: () => void
    onMilestoneChange?: () => Promise<void>
    onPreviewChange?: () => Promise<void>
    rollbackPoint?: string | null
    onExportEntry?: (entry: TimelineEntry) => void
    onRollback?: (entry: TimelineEntry) => void
    rollbackBusy?: boolean
  } = $props()

  let milestoneBusy = $state('')

  async function toggleMilestone(entry: TimelineEntry) {
    if (!activePath || milestoneBusy) return
    milestoneBusy = entry.id
    try {
      await setEntryMilestone(activePath, entry.id, !entry.milestone)
      await onMilestoneChange()
    } catch { /* ignore */ }
    milestoneBusy = ''
  }

  let previewBusy = $state('')

  async function togglePreview(entry: TimelineEntry) {
    if (!activePath || previewBusy || !entry.preview) return
    previewBusy = entry.id
    try {
      // 目标状态 = 当前状态取反（当前开启 → 关闭；当前关闭 → 开启）
      await setEntryPreview(activePath, entry.id, entry.preview_enabled === false)
      // [debug] 临时定位：调用成功
      console.log('[debug] setEntryPreview ok:', entry.id, 'target=', entry.preview_enabled === false)
      await onPreviewChange()
      console.log('[debug] onPreviewChange done:', entry.id)
    } catch (e) {
      console.log('[debug] setEntryPreview FAILED:', entry.id, e)
    }
    previewBusy = ''
  }

  let ctxMenu = $state<{ x: number; y: number; entry: TimelineEntry } | null>(null)

  function openCtx(e: MouseEvent, entry: TimelineEntry) {
    e.preventDefault()
    e.stopPropagation()
    ctxMenu = { x: e.clientX, y: e.clientY, entry }
  }

  function closeCtx() {
    ctxMenu = null
  }

  function ctxExport() {
    const m = ctxMenu
    closeCtx()
    if (m) onExportEntry(m.entry)
  }

  function ctxRollback() {
    const m = ctxMenu
    closeCtx()
    if (m) onRollback(m.entry)
  }

  function ctxTogglePreview() {
    const m = ctxMenu
    closeCtx()
    if (m && m.entry.preview) togglePreview(m.entry)
  }

  const ctxRollbackDisabled = $derived(rollbackBusy || (ctxMenu ? isEntryDimmed(ctxMenu.entry) : false))

  // 只有当前选中回退的记录（superseded_point = X）灰显不可回退；其余记录（含 X 之后、被顶替出主线的）均可回退。
  function isEntryDimmed(entry: TimelineEntry): boolean {
    return !!rollbackPoint && entry.id === rollbackPoint
  }

  let editingToken = $state(false)
  let tokenInput = $state('')
  let tokenSaving = $state(false)
  let copyFeedback = $state(false)
  let siteInput = $state('')
  let siteSaving = $state(false)

  $effect(() => { tokenInput = accessToken })
  $effect(() => { siteInput = siteName })

  let siteDirty = $derived(siteInput !== siteName)

  async function saveSiteName() {
    if (!activePath || !siteDirty) return
    siteSaving = true
    try {
      const ok = await onSiteNameChange(siteInput.trim())
      if (ok) siteInput = siteInput.trim()
    } catch { /* ignore */ }
    siteSaving = false
  }

  let isDirty = $derived(tokenInput !== accessToken)

  function generateToken(): string {
    const bytes = new Uint8Array(16)
    crypto.getRandomValues(bytes)
    return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')
  }

  async function saveToken() {
    if (!activePath || !isDirty) return
    tokenSaving = true
    const val = tokenInput.trim()
    try {
      await updateConfig(activePath, 'access_token', val || null)
      tokenInput = val
      if (onTokenChange) onTokenChange(val)
      editingToken = false
    } catch {
      /* ignore */
    }
    tokenSaving = false
  }

  function handleGenToken() {
    tokenInput = generateToken()
  }

  async function copyToken() {
    if (!accessToken) return
    try {
      await navigator.clipboard.writeText(accessToken)
      copyFeedback = true
      setTimeout(() => copyFeedback = false, 1500)
    } catch { /* ignore */ }
  }

  let expandedSections = $state<Record<string, Record<string, boolean>>>({})

  function hasSection(entryId: string, section: string): boolean {
    const e = entries.find(x => x.id === entryId)
    if (section === 'model' && e && e.files.some(f => f.endsWith('.blend'))) return expandedSections[entryId]?.[section] ?? true
    return expandedSections[entryId]?.[section] ?? false
  }

  function toggleSection(entryId: string, section: string) {
    const next = { ...expandedSections }
    if (!next[entryId]) next[entryId] = {}
    next[entryId] = { ...next[entryId], [section]: !next[entryId][section] }
    expandedSections = next
  }

  function statusIcon(s: string): string {
    if (s === 'A') return '+'
    if (s === 'D') return '−'
    if (s === 'M') return '~'
    return '·'
  }
  function statusClass(s: string): string {
    if (s === 'A') return 'st-add'
    if (s === 'D') return 'st-del'
    if (s === 'M') return 'st-mod'
    return ''
  }

  interface GroupedEntry {
    entry: TimelineEntry
    showGroup: boolean
    groupLabel: string
    groupColor: string
  }

  let grouped = $derived.by(() => {
    const result: GroupedEntry[] = []
    let lastTag: string | null | undefined = undefined
    for (const entry of entries) {
      if (entry.superseded) {
        // 被顶替记录：不参与 tag 分组，折叠组由模板按连续段渲染
        result.push({ entry, showGroup: false, groupLabel: '', groupColor: '' })
        continue
      }
      const entryTag = entry.tag || null
      const showGroup = entryTag !== lastTag
      lastTag = entryTag
      result.push({
        entry,
        showGroup,
        groupLabel: entryTag || '其他',
        groupColor: entryTag ? tagColor(entryTag) : '#6c7086',
      })
    }
    return result
  })

  // 被顶替记录折叠组（默认折叠）
  let supersededOpen = $state(false)
  const supersededCount = $derived(entries.filter((e) => e.superseded).length)
</script>

<div class="entries">
  <div class="entries-scroll">
    {#if entries.length === 0}
      <div class="empty-state">
        <div class="big"><svg viewBox="0 0 24 24" width="32" height="32" fill="currentColor" opacity="0.3"><path d="M12 2a10 10 0 100 20 10 10 0 000-20zm0 2a8 8 0 110 16 8 8 0 010-16zm-1 2v6.586l4.707 4.707 1.414-1.414L13 11.586V6h-2z"/></svg></div>
        <div>暂无记录</div>
        <div style="font-size:12px">使用记录按钮记录进度</div>
      </div>
    {:else}
      {#each grouped as g, i (g.entry.id)}
      {@const entry = g.entry}
      {@const entryTag = entry.tag || null}
      {@const prevEntry = grouped[i - 1]?.entry}
      {#if g.showGroup}
        <div class="tag-group">
          <span class="tag-group-dot" style="background:{g.groupColor}"></span>
          <span class="tag-group-label" style="color:{g.groupColor}">{g.groupLabel}</span>
          <span class="tag-group-line"></span>
        </div>
      {/if}
      {#if entry.superseded && !prevEntry?.superseded}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="superseded-header" onclick={() => (supersededOpen = !supersededOpen)}>
          <span class="arr">{supersededOpen ? '▾' : '▸'}</span>
          <span>被顶替记录（{supersededCount}）</span>
        </div>
      {/if}
      {#if !entry.superseded || supersededOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div
        class="entry-card"
        class:active={entry.id === selectedId}
        class:tagged={!!entryTag}
        class:milestone={!!entry.milestone}
        class:dimmed={isEntryDimmed(entry)}
        class:superseded={entry.superseded}
        style={`${entryTag ? `--tag-color:${tagColor(entryTag)}` : ''}--card-delay:${Math.min(i, 15) * 24}ms`}
        onclick={(e) => { e.stopPropagation(); onSelect(entry.id); }}
        oncontextmenu={(e) => openCtx(e, entry)}
      >
        <button
          class="ms-badge"
          class:active={!!entry.milestone}
          title={entry.milestone ? '取消里程碑' : '标记为里程碑'}
          onclick={(e) => { e.stopPropagation(); toggleMilestone(entry); }}
        >
          <svg viewBox="0 0 16 16" width="10" height="10" fill="currentColor"><path d="M8 .6l1.9 4.6 4.9.4-3.7 3.2 1.1 4.8L8 11.2 3.8 13.6l1.1-4.8L1.2 5.6l4.9-.4L8 .6z"/></svg>
        </button>
        <div class="top">
          <span class="hash">{entry.id.slice(0, 7)}</span>
          <span class="top-right">
            {#if entry.preview}
              <span
                class="preview-badge"
                class:active={entry.preview_enabled !== false}
                title={entry.preview_enabled === false ? '3D 预览已关闭（右键记录可开启）' : '该记录有 3D 预览（右键记录可关闭）'}
              >3D</span>
            {/if}
            <span class="date">{entry.timestamp.slice(0, 19).replace('T', ' ')}</span>
          </span>
        </div>
        <div class="msg">
          {entry.message}
          {#if entryTag}
            <span class="tag-badge" style="--tag-c:{tagColor(entryTag)}">{entryTag}</span>
          {/if}
        </div>

        <!-- Section 1: File Changes -->
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="section-header" onclick={(e) => { e.stopPropagation(); toggleSection(entry.id, 'files'); }}>
          <span class="arr">{hasSection(entry.id, 'files') ? '▾' : '▸'}</span>
          <span>文件变动</span>
        </div>
        {#if hasSection(entry.id, 'files')}
          <div class="section-body">
            {#each entry.files as f, i}
              {@const st = entry.file_status?.[i] || ''}
              <div class="file-status-line {statusClass(st)}">
                <span class="status-badge {statusClass(st)}">{statusIcon(st)}</span>
                <span class="status-path">{f}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
      {/if}
    {/each}
    {/if}
  </div>
  {#if ctxMenu}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="ctx-overlay" onclick={closeCtx} oncontextmenu={(e) => { e.preventDefault(); closeCtx(); }}></div>
    <div class="ctx-menu" style="left:{ctxMenu.x}px;top:{ctxMenu.y}px">
      <button class="ctx-item" onclick={ctxExport}>导出</button>
      {#if ctxMenu.entry.preview}
        <button class="ctx-item" onclick={ctxTogglePreview} disabled={previewBusy === ctxMenu.entry.id}>
          {ctxMenu.entry.preview_enabled === false ? '开启 3D 预览' : '关闭 3D 预览'}
        </button>
      {/if}
      <button class="ctx-item" onclick={ctxRollback} disabled={ctxRollbackDisabled} title={rollbackPoint ? '该记录为当前回退点，不可回退' : ''}>回退</button>
    </div>
  {/if}
  <div class="entries-footer">
    <div class="at-section">
      <label for="el-site-name">网页名称</label>
      {#if siteLocked}
        <div class="at-row">
          <div class="at-masked site-name-display" title={siteName}>{siteName}</div>
          <button class="at-save-btn site-open-btn" onclick={onOpenSite} title="在浏览器中打开部署的网页">
            <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M6.354 5.5H4a3 3 0 0 0 0 6h3a3 3 0 0 0 2.83-4H9c-.086 0-.17.01-.25.031a2 2 0 0 1-2.29 2.969A2 2 0 0 1 4 8.5v-.5h2.354a1 1 0 0 0 0-2zM12 4.5H9a3 3 0 0 0-2.83 4h1.049a2 2 0 0 1 2.29-2.969A2 2 0 0 1 12 7.5v.5H9.646a1 1 0 0 0 0 2H12a3 3 0 0 0 0-6z"/></svg>
            打开网页
          </button>
        </div>
      {:else}
        <div class="at-row">
          <input
            type="text"
            id="el-site-name"
            class="at-input"
            bind:value={siteInput}
            placeholder="站点名（仅限英文、数字、- 和 _）"
            onkeydown={(e) => { if (e.key === 'Enter') saveSiteName() }}
          />
          <button
            class="at-save-btn"
            class:saved={!siteDirty}
            onclick={saveSiteName}
            disabled={siteSaving || !siteDirty}
          >
            {siteSaving ? '保存中...' : siteDirty ? '保存' : '已保存'}
          </button>
        </div>
        <div class="site-hint">部署后锁定，作为站点访问地址</div>
      {/if}
    </div>
    {#if siteLocked}
      <div class="at-section">
        <label for="el-access-token">网页访问令牌</label>
        <div class="at-row">
          {#if editingToken}
            <input
              type="text"
              id="el-access-token"
              class="at-input"
              bind:value={tokenInput}
              placeholder="访问令牌"
            />
            {#if !tokenInput}
              <button class="at-gen-btn" onclick={handleGenToken}>随机生成</button>
            {/if}
            <button
              class="at-save-btn"
              class:saved={!isDirty}
              onclick={saveToken}
              disabled={tokenSaving || !isDirty}
            >
              {tokenSaving ? '保存中...' : isDirty ? '保存' : '已保存'}
            </button>
          {:else if accessToken}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div
              class="at-masked"
              class:copied={copyFeedback}
              onclick={copyToken}
              ondblclick={() => { editingToken = true; tokenInput = accessToken }}
              title="单击复制，双击编辑"
            >
              {#if copyFeedback}
                已复制!
              {:else}
                {'*'.repeat(Math.min(accessToken.length, 16))}
              {/if}
            </div>
          {:else}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="at-empty" onclick={() => { editingToken = true; tokenInput = '' }} title="点击设置访问令牌">
              - 未设置 -
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .top-right {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .entry-card {
    position: relative;
  }
  .ms-badge {
    position: absolute;
    top: -6px;
    left: -6px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-dim);
    cursor: pointer;
    padding: 0;
    opacity: 0;
    visibility: hidden;
    transform: scale(0.8);    transition: opacity 0.15s ease, transform 0.15s ease, visibility 0.15s, background 0.15s, color 0.15s, border-color 0.15s;
    box-shadow: var(--shadow-sm);
    z-index: 2;
  }
  .entry-card:hover .ms-badge,
  .entry-card.milestone .ms-badge {
    opacity: 1;
    visibility: visible;
    transform: scale(1);
  }
  .ms-badge:hover {
    color: var(--accent);
    border-color: var(--accent);
  }
  .ms-badge.active {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .ms-badge.active:hover {
    background: var(--accent-hover);
    border-color: var(--accent-hover);
    color: #fff;
  }
  .preview-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--surface2);
    border: 1px solid var(--border);
    color: var(--text-dim);
    margin-right: 6px;
    vertical-align: 1px;
    user-select: none;
  }
  .preview-badge.active {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .preview-badge:not(.active) {
    opacity: 0.55;
    text-decoration: line-through;
  }
  .superseded-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    margin: 4px 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
    background: var(--surface);
    border: 1px dashed var(--border);
    border-radius: 6px;
    cursor: pointer;
    user-select: none;
  }
  .superseded-header:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .superseded-header .arr {
    font-size: 9px;
  }
  .entry-card.milestone {
    background: color-mix(in srgb, var(--accent) 11%, var(--surface));
    border-color: var(--accent);
    border-left: 5px solid var(--accent);
    box-shadow: 0 2px 16px color-mix(in srgb, var(--accent) 25%, transparent);
  }
  .entry-card.milestone:hover {
    border-color: var(--accent);
  }
  .entry-card.milestone.active {
    outline-offset: -1px;
    box-shadow: 0 2px 18px color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .entry-card.milestone .msg {
    font-weight: 600;
  }
  /* 被顶替记录（superseded）：整卡变灰（置于 milestone 之后，覆盖其底色；与 dimmed 叠加自然） */
  .entry-card.superseded {
    background: color-mix(in srgb, var(--text-dim) 8%, var(--surface));
    border-color: var(--border);
    color: var(--text-dim);
    filter: saturate(0.3);
  }
  .entry-card.superseded:hover {
    border-color: var(--text-dim);
  }
  .entry-card.superseded .hash,
  .entry-card.superseded .date {
    color: var(--text-dim);
  }
  .section-header {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: 6px;
    padding: 5px 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
    cursor: pointer;
    user-select: none;
    border-top: 1px solid var(--border);
    transition: color 0.12s;
  }
  .section-header:hover { color: var(--accent); }
  .section-header .arr { font-size: 9px; width: 10px; flex-shrink: 0; }
  .section-body { padding: 4px 0 2px 15px; font-size: 12px; }
  .file-status-line { display: flex; align-items: center; gap: 6px; padding: 2px 0; font-family: var(--mono); font-size: 12px; }
  .status-badge {
    width: 18px; height: 18px;
    display: inline-flex; align-items: center; justify-content: center;
    border-radius: 999px;
    font-weight: 700; font-size: 12px;
    flex-shrink: 0;
  }
  .status-path { color: var(--text); }
  .st-add { color: var(--success); background: var(--success-soft); }
  .st-del { color: var(--error); background: var(--accent-soft); }
  .st-mod { color: var(--warning); background: var(--warning-soft); }

  .model-entry { margin-bottom: 6px; }

  .model-err { font-size: 11px; color: var(--text-dim); padding: 8px; text-align: center; background: var(--surface2); border-radius: 6px; }
  .open-web-btn {
    margin-top: 6px;
    padding: 5px 14px;
    border-radius: 8px;
    border: 1px solid var(--accent);
    background: transparent;
    color: var(--accent);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
  }
  .open-web-btn:hover {
    background: var(--accent);
    color: #fff;
  }
  .tag-group { display: flex; align-items: center; gap: 8px; margin: 12px 0 6px; }
  .tag-group-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; box-shadow: 0 0 0 3px var(--surface); }
  .tag-group-label { font-size: 12px; font-weight: 700; }
  .tag-group-line { flex: 1; height: 1px; background: var(--border); }
  .tag-badge {
    display: inline-block;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 999px;
    margin-left: 6px;
    vertical-align: middle;
    color: color-mix(in srgb, var(--tag-c) 45%, var(--text));
    background: color-mix(in srgb, var(--tag-c) 14%, transparent);
  }
  .entries-footer {
    flex-shrink: 0;
    padding: 10px 10px 6px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }
  .at-section label {
    display: block;
    font-size: 11px;
    color: var(--text-dim);
    margin-bottom: 4px;
  }
  .at-row {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .at-input {
    flex: 1;
    font-size: 11px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    font-family: var(--mono);
  }
  .at-masked {
    flex: 1;
    font-size: 11px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text-dim);
    cursor: pointer;
    user-select: none;
    transition: background 0.12s, border-color 0.12s;
    font-family: var(--mono);
  }
  .at-masked:hover {
    background: var(--surface2);
    border-color: var(--accent);
  }
  .at-masked.copied {
    color: var(--success);
    border-color: var(--success);
  }
  .at-empty {
    flex: 1;
    font-size: 11px;
    padding: 4px 8px;
    border: 1px dashed var(--border);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text-dim);
    cursor: pointer;
    user-select: none;
    text-align: center;
    transition: background 0.12s, border-color 0.12s, color 0.12s;
  }
  .at-empty:hover {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .at-gen-btn {
    font-size: 10px;
    padding: 3px 10px;
    border-radius: 6px;
    background: var(--surface);
    border: 1px dashed var(--accent);
    color: var(--accent);
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s;
  }
  .at-gen-btn:hover {
    background: var(--accent-soft);
  }
  .at-save-btn {
    font-size: 10px;
    padding: 3px 10px;
    border-radius: 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s;
  }
  .at-save-btn:hover:not(:disabled) {
    background: var(--surface2);
  }
  .at-save-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .at-save-btn.saved {
    opacity: 0.5;
    cursor: default;
    pointer-events: none;
  }
  .site-name-display {
    flex: 1;
    font-family: var(--mono);
    cursor: default;
  }
  .site-name-display:hover {
    background: var(--surface);
    border-color: var(--border);
  }
  .site-open-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: var(--accent) !important;
    border-color: var(--accent) !important;
    color: #fff !important;
    font-weight: 600;
  }
  .site-open-btn:hover:not(:disabled) {
    background: var(--accent-hover) !important;
    border-color: var(--accent-hover) !important;
  }
  .site-hint {
    font-size: 10px;
    color: var(--text-dim);
    margin-top: 4px;
  }
  .entry-card.dimmed {
    opacity: 0.45;
    filter: saturate(0.6);
  }
  .ctx-overlay {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .ctx-menu {
    position: fixed;
    z-index: 41;
    min-width: 120px;
    background: var(--surface2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 4px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }
  .ctx-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 6px 10px;
    font-size: 12px;
    border-radius: 6px;
    background: transparent;
    border: none;
    color: var(--text);
    cursor: pointer;
  }
  .ctx-item:hover:not(:disabled) {
    background: var(--accent);
    color: #fff;
  }
  .ctx-item:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
