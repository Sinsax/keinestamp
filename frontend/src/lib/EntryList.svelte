<script lang="ts">
  import DirTree from './DirTree.svelte'
  import type { TimelineEntry } from '../types'
  import { getFileDiff, openWebTimeline, updateConfig } from '../api'

  let {
    entries,
    selectedId,
    activePath = '',
    onSelect,
    onDblClick,
    accessToken = '',
    onTokenChange,
  }: {
    entries: TimelineEntry[]
    selectedId: string
    activePath?: string
    onSelect: (id: string) => void
    onDblClick: (entry: TimelineEntry) => void
    accessToken?: string
    onTokenChange?: (val: string) => void
  } = $props()

  let editingToken = $state(false)
  let tokenInput = $state(accessToken)
  let tokenSaving = $state(false)
  let copyFeedback = $state(false)

  $effect(() => { tokenInput = accessToken })

  let isDirty = $derived(tokenInput !== accessToken)

  let editTimer: ReturnType<typeof setTimeout> | undefined

  $effect(() => {
    if (!editingToken) return
    tokenInput
    if (editTimer) clearTimeout(editTimer)
    editTimer = setTimeout(() => { editingToken = false }, 5000)
    return () => { if (editTimer) clearTimeout(editTimer) }
  })

  function generateToken(): string {
    const bytes = new Uint8Array(16)
    crypto.getRandomValues(bytes)
    return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')
  }

  async function saveToken() {
    if (!activePath || !isDirty) return
    tokenSaving = true
    try {
      await updateConfig(activePath, 'access_token', tokenInput || null)
      if (onTokenChange) onTokenChange(tokenInput)
      editingToken = false
    } catch { /* ignore */ }
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
  let diffs = $state<Record<string, Record<string, string>>>({})
  let diffLoading = $state<Record<string, boolean>>({})

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
    if (next[entryId][section]) {
      if (section === 'diff') loadDiffs(entryId)
    }
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

  let textFiles = $derived((entryId: string) => {
    const e = entries.find(x => x.id === entryId)
    if (!e) return []
    return e.files.filter((f, i) => !f.endsWith('.blend') && e.file_status?.[i] !== 'A')
  })

  let blendFiles = $derived((entryId: string) => {
    const e = entries.find(x => x.id === entryId)
    if (!e) return []
    return e.files.filter(f => f.endsWith('.blend'))
  })

  const TAG_COLORS: Record<string, string> = {
    '建模': '#89b4fa',
    'UV拆分': '#a6e3a1',
    '绑定': '#cba6f7',
    '贴图': '#fab387',
    '修复': '#f38ba8',
  }
  const FALLBACK_COLORS = ['#89b4fa', '#a6e3a1', '#cba6f7', '#fab387', '#f38ba8', '#94e2d5']

  function tagColor(tag: string): string {
    if (TAG_COLORS[tag]) return TAG_COLORS[tag]
    let hash = 0
    for (let i = 0; i < tag.length; i++) hash = ((hash << 5) - hash) + tag.charCodeAt(i)
    return FALLBACK_COLORS[Math.abs(hash) % FALLBACK_COLORS.length]
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

  interface DiffLine {
    lineNum: number
    type: 'add' | 'del' | 'mod'
    text: string
  }

  function parseDiff(diffText: string, fileStatus: string): DiffLine[] {
    if (fileStatus === 'A') return []
    const result: DiffLine[] = []
    let newLineNum = 0
    let oldLineNum = 0
    let hunkHasDel = false
    const hunkBuf: { prefix: string; text: string }[] = []

    function flushHunk() {
      for (const h of hunkBuf) {
        if (h.prefix === '+') {
          result.push({ lineNum: newLineNum, type: hunkHasDel ? 'mod' : 'add', text: h.text })
          newLineNum++
        } else if (h.prefix === '-') {
          result.push({ lineNum: oldLineNum, type: 'del', text: h.text })
          oldLineNum++
        }
        if (h.prefix === ' ') { newLineNum++; oldLineNum++ }
      }
      hunkBuf.length = 0
      hunkHasDel = false
    }

    for (const rawLine of diffText.split('\n')) {
      const m = rawLine.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/)
      if (m) {
        flushHunk()
        oldLineNum = parseInt(m[1])
        newLineNum = parseInt(m[2])
        continue
      }
      if (rawLine.startsWith('diff --git') || rawLine.startsWith('index ') || rawLine.startsWith('---') || rawLine.startsWith('+++')) continue
      if (rawLine.startsWith('+') || rawLine.startsWith('-') || rawLine.startsWith(' ')) {
        const prefix = rawLine[0]
        if (prefix === '-') hunkHasDel = true
        if (prefix !== ' ') hunkBuf.push({ prefix, text: rawLine.slice(1) })
        else { newLineNum++; oldLineNum++ }
      }
    }
    flushHunk()
    return result
  }

  function diffLines(entryId: string, file: string, status: string): DiffLine[] {
    const raw = diffs[entryId]?.[file]
    if (!raw) return []
    return parseDiff(raw, status)
  }

  async function loadDiffs(entryId: string) {
    if (diffLoading[entryId]) return
    diffLoading = { ...diffLoading, [entryId]: true }
    const e = entries.find(x => x.id === entryId)
    if (!e || !activePath) { diffLoading = { ...diffLoading, [entryId]: false }; return }
    const files = e.files.filter((f, i) => !f.endsWith('.blend') && e.file_status?.[i] !== 'A')
    const results: Record<string, string> = {}
    for (const f of files) {
      try {
        results[f] = await getFileDiff(activePath, entryId, f)
      } catch { /* ignore */ }
    }
    diffs = { ...diffs, [entryId]: results }
    diffLoading = { ...diffLoading, [entryId]: false }
  }
</script>

<div class="entries">
  {#if entries.length === 0}
    <div class="empty-state">
      <div class="big">~</div>
<div>暂无记录</div>
              <div style="font-size:12px">使用记录按钮记录进度</div>
    </div>
  {:else}
    {#each grouped as g (g.entry.id)}
      {@const entry = g.entry}
      {@const entryTag = entry.tag || null}
      {@const textF = entry.files.map((f, i) => ({ file: f, status: entry.file_status?.[i] || '', idx: i })).filter(x => !x.file.endsWith('.blend') && x.status !== 'A')}
      {@const blendF = entry.files.filter(f => f.endsWith('.blend'))}
      {#if g.showGroup}
        <div class="tag-group">
          <span class="tag-group-dot" style="background:{g.groupColor}"></span>
          <span class="tag-group-label" style="color:{g.groupColor}">{g.groupLabel}</span>
          <span class="tag-group-line"></span>
        </div>
      {/if}
      <div
        class="entry-card"
        class:active={entry.id === selectedId}
        class:tagged={!!entryTag}
        style={entryTag ? `--tag-color:${tagColor(entryTag)}` : ''}
        onclick={(e) => { e.stopPropagation(); onSelect(entry.id); }}
        ondblclick={(e) => { e.stopPropagation(); onDblClick(entry); }}
      >
        <div class="top">
          <span class="hash">{entry.id.slice(0, 7)}</span>
          <span class="date">{entry.timestamp.slice(0, 19).replace('T', ' ')}</span>
        </div>
        <div class="msg">
          {entry.message}
          {#if entryTag}
            <span class="tag-badge" style="background:{tagColor(entryTag)}">{entryTag}</span>
          {/if}
        </div>

        <!-- Section 1: File Changes -->
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

        <!-- Section 2: Text Diffs -->
        {#if textF.length > 0}
          <div class="section-header" onclick={(e) => { e.stopPropagation(); toggleSection(entry.id, 'diff'); }}>
            <span class="arr">{hasSection(entry.id, 'diff') ? '▾' : '▸'}</span>
            <span>文本改动 ({textF.length})</span>
          </div>
          {#if hasSection(entry.id, 'diff')}
            <div class="section-body">
              {#if diffLoading[entry.id]}
                <div class="section-loading">加载中...</div>
              {:else}
                {#each textF as tf}
                  {@const lines = diffLines(entry.id, tf.file, tf.status)}
                  <div class="diff-entry">
                    <div class="diff-path">{tf.file}</div>
                    {#if lines.length > 0}
                      {#each lines as ln}
                        <div class="dl {ln.type}">
                          <span class="dl-num">{ln.lineNum}</span>
                          <span class="dl-text">{ln.text}</span>
                        </div>
                      {/each}
                    {:else}
                      <div class="diff-empty">无差异</div>
                    {/if}
                  </div>
                {/each}
              {/if}
            </div>
          {/if}
        {/if}

        <!-- Section 3: Blend Models -->
        {#if blendF.length > 0}
          <div class="section-header" onclick={(e) => { e.stopPropagation(); toggleSection(entry.id, 'model'); }}>
            <span class="arr">{hasSection(entry.id, 'model') ? '▾' : '▸'}</span>
            <span>3D 模型预览 ({blendF.length})</span>
          </div>
          {#if hasSection(entry.id, 'model')}
            <div class="section-body">
              {#each blendF as f}
                <div class="diff-path" style="margin-bottom:4px">{f}</div>
              {/each}
              <button class="open-web-btn" onclick={(e) => { e.stopPropagation(); if (activePath) openWebTimeline(activePath) }}>
                在浏览器中打开 — 3D 预览
              </button>
            </div>
          {/if}
        {/if}
      </div>
    {/each}
  {/if}
  <div class="entries-footer">
    <div class="at-section">
      <label>网页访问令牌</label>
      <div class="at-row">
        {#if editingToken}
          <input
            type="text"
            class="at-input"
            bind:value={tokenInput}
            placeholder="访问令牌"
            autofocus
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
          <div class="at-empty" onclick={() => { editingToken = true; tokenInput = '' }} title="点击设置访问令牌">
            - 未设置 -
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .section-header {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: 6px;
    padding: 4px 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
    cursor: pointer;
    user-select: none;
    border-top: 1px solid var(--border);
    padding-top: 5px;
  }
  .section-header:hover { color: var(--text); }
  .section-header .arr { font-size: 9px; width: 10px; flex-shrink: 0; }
  .section-body { padding: 4px 0 2px 15px; font-size: 12px; }
  .section-loading { color: var(--text-dim); font-size: 11px; padding: 4px 0; }
  .file-status-line { display: flex; align-items: center; gap: 6px; padding: 1px 0; font-family: monospace; font-size: 12px; }
  .status-badge { width: 18px; text-align: center; font-weight: 700; font-size: 13px; flex-shrink: 0; }
  .status-path { color: var(--text); }
  .st-add { color: #4caf88; }
  .st-del { color: #e06c75; }
  .st-mod { color: #e5c07b; }

  .diff-entry, .model-entry { margin-bottom: 6px; }
  .diff-path {
    font-size: 11px; font-family: monospace;
    color: var(--accent); padding: 2px 6px;
    background: rgba(0,0,0,0.06); border-radius: 3px; margin-bottom: 3px;
  }
  .diff-empty { font-size: 11px; color: var(--text-dim); padding: 2px 6px; }

  .dl {
    display: flex;
    gap: 8px;
    font-family: monospace;
    font-size: 11px;
    line-height: 1.6;
    padding: 0 4px;
    border-radius: 2px;
  }
  .dl-num {
    width: 32px;
    text-align: right;
    flex-shrink: 0;
    color: var(--text-dim);
    font-size: 10px;
    user-select: none;
  }
  .dl-text { white-space: pre; flex: 1; }
  .dl.add { background: rgba(76, 175, 136, 0.12); color: #4caf88; }
  .dl.add .dl-num { color: #4caf88; }
  .dl.del { background: rgba(224, 108, 117, 0.12); color: #e06c75; }
  .dl.del .dl-num { color: #e06c75; }
  .dl.mod { background: rgba(100, 140, 255, 0.12); color: #648cff; }
  .dl.mod .dl-num { color: #648cff; }

  .model-err { font-size: 11px; color: var(--text-dim); padding: 8px; text-align: center; background: var(--surface); border-radius: 6px; }
  .open-web-btn {
    margin-top: 6px;
    padding: 5px 14px;
    border-radius: 6px;
    border: 1px solid var(--accent, #cba6f7);
    background: transparent;
    color: var(--accent, #cba6f7);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .open-web-btn:hover {
    background: var(--accent, #cba6f7);
    color: #1e1e2e;
  }
  .tag-group { display: flex; align-items: center; gap: 8px; margin: 10px 0 4px; }
  .tag-group-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .tag-group-label { font-size: 12px; font-weight: 700; }
  .tag-group-line { flex: 1; height: 1px; background: var(--border); }
  .tag-badge {
    display: inline-block;
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 8px;
    margin-left: 6px;
    vertical-align: middle;
    color: #1e1e2e;
  }
  .entries-footer {
    padding: 10px 0 4px;
    border-top: 1px solid var(--border);
    margin-top: 8px;
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
    font-family: monospace;
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
    font-family: monospace;
  }
  .at-masked:hover {
    background: var(--surface2);
    border-color: var(--accent, #cba6f7);
  }
  .at-masked.copied {
    color: #4ade80;
    border-color: #4ade80;
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
    transition: background 0.12s, border-color 0.12s;
  }
  .at-empty:hover {
    background: var(--surface);
    border-color: var(--accent, #cba6f7);
    color: var(--accent, #cba6f7);
  }
  .at-gen-btn {
    font-size: 10px;
    padding: 3px 10px;
    border-radius: 6px;
    background: var(--surface);
    border: 1px dashed var(--accent, #cba6f7);
    color: var(--accent, #cba6f7);
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.12s;
  }
  .at-gen-btn:hover {
    background: rgba(203, 166, 247, 0.12);
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

</style>
