<script lang="ts">
  import DirTree from './DirTree.svelte'
  import type { TimelineEntry } from '../types'
  import { openWebTimeline } from '../api'

  const TAG_COLORS: Record<string, string> = {
    '建模': '#3d6fa8',
    'UV拆分': '#3e6b52',
    '绑定': '#7a5fa8',
    '贴图': '#a8722e',
    '修复': '#c03535',
  }
  const FALLBACK_COLORS = ['#3d6fa8', '#3e6b52', '#7a5fa8', '#a8722e', '#c03535', '#4e8a8a']

  function tagColor(tag: string): string {
    if (TAG_COLORS[tag]) return TAG_COLORS[tag]
    let hash = 0
    for (let i = 0; i < tag.length; i++) hash = ((hash << 5) - hash) + tag.charCodeAt(i)
    return FALLBACK_COLORS[Math.abs(hash) % FALLBACK_COLORS.length]
  }

  function tagSoft(tag: string): string {
    const c = tagColor(tag)
    const r = parseInt(c.slice(1, 3), 16)
    const g = parseInt(c.slice(3, 5), 16)
    const b = parseInt(c.slice(5, 7), 16)
    return `rgba(${r},${g},${b},0.12)`
  }

  let {
    entry,
    activePath = '',
    onClose,
  }: {
    entry: TimelineEntry | null
    activePath?: string
    onClose: () => void
  } = $props()

  let blendFiles = $derived(entry ? entry.files.filter(f => f.endsWith('.blend')) : [])
  let openError = $state('')

  async function handleOpenWeb() {
    if (!activePath) return
    openError = ''
    try {
      await openWebTimeline(activePath)
    } catch (e) {
      openError = `${e}`
    }
  }
</script>

{#if entry}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="detail-overlay" onclick={onClose} role="dialog">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="detail-panel" onclick={(e: MouseEvent) => e.stopPropagation()} role="document">
      <h2>{entry.message}
        {#if entry.tag}
          <span class="tag-badge" style="color:{tagColor(entry.tag)};background:{tagSoft(entry.tag)}">{entry.tag}</span>
        {/if}
      </h2>
      <div class="meta">{entry.id.slice(0, 7)} &mdash; {entry.author}</div>

      <div class="field">
        <label>时间戳</label>
        <div class="value">{entry.timestamp.slice(0, 19).replace('T', ' ')}</div>
      </div>

      <div class="field">
        <label>文件 ({entry.files.length})</label>
        <div class="value ft">
          <DirTree paths={entry.files} />
        </div>
      </div>

      {#if blendFiles.length > 0}
        <div class="field">
          <label>3D 模型</label>
          <div class="value" style="margin-top:4px">
            {#each blendFiles as f}
              <div class="blend-item">{f}</div>
            {/each}
            <button class="open-web-btn" onclick={handleOpenWeb}>
              在浏览器中打开 — 3D 预览
            </button>
            {#if openError}
              <div class="open-err">{openError}</div>
            {/if}
          </div>
        </div>
      {/if}

      <div style="display:flex;gap:8px;justify-content:flex-end;margin-top:16px">
        <button onclick={onClose}>关闭</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .ft { max-height: 200px; overflow-y: auto; }
  .tag-badge {
    display: inline-block;
    font-size: 11px;
    font-weight: 600;
    padding: 2px 10px;
    border-radius: 999px;
    margin-left: 8px;
    vertical-align: middle;
  }
  .blend-item { font-family: var(--mono); font-size: 11px; color: var(--blue); margin-bottom: 4px; }
  .open-web-btn {
    margin-top: 8px;
    padding: 6px 16px;
    border-radius: 8px;
    border: 1px solid var(--accent);
    background: transparent;
    color: var(--accent);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
  }
  .open-web-btn:hover {
    background: var(--accent);
    color: #fff;
  }
  .open-err { color: var(--error); font-size: 11px; margin-top: 4px; }
</style>