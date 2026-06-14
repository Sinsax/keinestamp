<script lang="ts">
  import DirTree from './DirTree.svelte'
  import type { TimelineEntry } from '../types'
  import { openWebTimeline } from '../api'

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
          <span class="tag-badge" style="background:{tagColor(entry.tag)}">{entry.tag}</span>
        {/if}
      </h2>
      <div class="meta">{entry.id.slice(0, 7)} &mdash; {entry.author}</div>

      <div class="field">
        <label>Timestamp</label>
        <div class="value">{entry.timestamp.slice(0, 19).replace('T', ' ')}</div>
      </div>

      <div class="field">
        <label>Files ({entry.files.length})</label>
        <div class="value ft">
          <DirTree paths={entry.files} />
        </div>
      </div>

      {#if blendFiles.length > 0}
        <div class="field">
          <label>3D Model</label>
          <div class="value" style="margin-top:4px">
            {#each blendFiles as f}
              <div class="blend-item">{f}</div>
            {/each}
            <button class="open-web-btn" onclick={handleOpenWeb}>
              Open in Browser — 3D Preview
            </button>
            {#if openError}
              <div class="open-err">{openError}</div>
            {/if}
          </div>
        </div>
      {/if}

      <div style="display:flex;gap:8px;justify-content:flex-end;margin-top:16px">
        <button onclick={onClose}>Close</button>
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
    padding: 1px 8px;
    border-radius: 10px;
    margin-left: 8px;
    vertical-align: middle;
    color: #1e1e2e;
  }
  .blend-item { font-family: monospace; font-size: 11px; color: var(--accent); margin-bottom: 4px; }
  .open-web-btn {
    margin-top: 8px;
    padding: 6px 16px;
    border-radius: 6px;
    border: 1px solid var(--accent, #cba6f7);
    background: transparent;
    color: var(--accent, #cba6f7);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .open-web-btn:hover {
    background: var(--accent, #cba6f7);
    color: #1e1e2e;
  }
  .open-err { color: var(--error, #f38ba8); font-size: 11px; margin-top: 4px; }
</style>