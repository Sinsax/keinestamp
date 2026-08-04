<script lang="ts">
  import DirTree from './DirTree.svelte'
  import type { TimelineEntry } from '../types'
  import { openWebTimeline } from '../api'
  import { tagColor, tagSoft } from './tags'

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
  .detail-panel {
    width: 560px;
    max-width: min(560px, 90vw);
    max-height: 80vh;
    overflow-y: auto;
  }
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