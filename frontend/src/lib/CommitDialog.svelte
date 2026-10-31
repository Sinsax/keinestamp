<script lang="ts">
  import type { TagGroup } from '../api'
  import { resolveTagCategory } from './tags'

  let {
    onConfirm,
    onCancel,
    lastTag = '',
    lastTags = [] as string[],
    lastTagCategory = '',
    tagGroups = [] as TagGroup[],
    busy = false,
    progress = '',
    hasPreviewMarked = false,
  }: {
    onConfirm: (msg: string, tag: string | null, tagCategory: string | null, tags: string[], noPreview: boolean) => Promise<boolean>
    onCancel: () => void
    lastTag?: string
    lastTags?: string[]
    lastTagCategory?: string
    tagGroups?: TagGroup[]
    busy?: boolean
    progress?: string
    /** 本次选择文件中是否有标记预览的文件（无标记则不显示预览选框） */
    hasPreviewMarked?: boolean
  } = $props()

  // 仅后端读取失败时兜底：默认只有一个 Blender 流程（原「建模流程」改名）
  const DEFAULT_TAG_GROUPS: TagGroup[] = [
    { id: 'blender', label: 'Blender', tags: ['建模', 'UV拆分', '绑定', '贴图', '修复'] },
  ]
  const TAG_GROUPS = $derived(tagGroups.length > 0 ? tagGroups : DEFAULT_TAG_GROUPS)

  let message = $state('')
  let category = $state('')
  let selectedTags = $state<string[]>([])
  let customTag = $state('')
  let useCustom = $state(false)
  let previewEnabled = $state(true)

  const currentTags = $derived(TAG_GROUPS.find((g) => g.id === category)?.tags ?? [])

  function effectiveTags(): string[] {
    if (useCustom) {
      const t = customTag.trim()
      return t ? [t] : []
    }
    return selectedTags
  }

  function effectiveTag(): string | null {
    const tags = effectiveTags()
    return tags.length > 0 ? tags[0] : null
  }

  function effectiveCategory(): string | null {
    return effectiveTags().length > 0 ? category : null
  }

  async function handleConfirm() {
    if (busy || !message.trim()) return
    const ok = await onConfirm(message.trim(), effectiveTag(), effectiveCategory(), effectiveTags(), !previewEnabled)
    if (ok) onCancel()
  }

  $effect(() => {
    const hasLast = lastTag || (lastTags && lastTags.length > 0)
    // 文件类型建议只作为提示，不再自动选中流程/标签，避免“勾了 png 就被迫选绘画流程”
    if (!hasLast) return
    const lastCategory = resolveTagCategory(lastTagCategory)
    if (lastCategory) {
      const known = TAG_GROUPS.some((g) => g.id === lastCategory)
      if (known) category = lastCategory
    }
    if (lastTags && lastTags.length > 0) {
      const group = TAG_GROUPS.find((g) => g.id === lastCategory) ?? TAG_GROUPS[0]
      selectedTags = lastTags.filter((t) => group.tags.includes(t))
      useCustom = false
    } else if (lastTag) {
      const group = TAG_GROUPS.find((g) => g.id === lastCategory) ?? TAG_GROUPS[0]
      if (group.tags.includes(lastTag)) {
        selectedTags = [lastTag]
        useCustom = false
      } else {
        customTag = lastTag
        useCustom = true
      }
    }
  })

  function selectCategory(id: string) {
    category = id
    if (!useCustom) {
      selectedTags = selectedTags.filter((t) => currentTags.includes(t))
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) handleConfirm()
    if (e.key === 'Escape' && !busy) onCancel()
  }

  function toggleTag(p: string) {
    useCustom = false
    if (selectedTags.includes(p)) {
      selectedTags = selectedTags.filter((t) => t !== p)
    } else {
      selectedTags = [...selectedTags, p]
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="dialog-overlay" onclick={() => { if (!busy) onCancel() }} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog commit-dialog" onclick={(e: MouseEvent) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>记录进度</h3>
    <div class="field">
      <label for="cd-message">记录说明</label>
      <textarea
        id="cd-message"
        bind:value={message}
        rows={3}
        placeholder="描述本次修改..."
      ></textarea>
    </div>
    <div class="field">
      <span class="cat-label">流程分类（可选）</span>
      <div class="cat-row">
        {#each TAG_GROUPS as g}
          <button
            class="cat-btn"
            class:active={category === g.id}
            onclick={() => selectCategory(g.id)}
          >{g.label}</button>
        {/each}
      </div>
    </div>
    <div class="field">
      <label for="cd-tag">标签（可选）</label>
      <div class="tag-row">
        {#each currentTags as p}
          <button
            class="tag-btn"
            class:active={selectedTags.includes(p) && !useCustom}
            onclick={() => toggleTag(p)}
          >{p}</button>
        {/each}
        <button
          class="tag-btn custom-btn"
          class:active={useCustom}
          onclick={() => { useCustom = true; selectedTags = []; }}
        >自定义</button>
      </div>
      {#if useCustom}
        <input
          id="cd-tag"
          class="custom-input"
          type="text"
          bind:value={customTag}
          placeholder="输入自定义标签..."
        />
      {/if}
    </div>
    {#if hasPreviewMarked}
      <div class="field row-field">
        <label for="cd-preview">生成预览</label>
        <input id="cd-preview" type="checkbox" bind:checked={previewEnabled} />
      </div>
    {/if}
    <div class="actions">
      <button onclick={onCancel} disabled={busy}>取消</button>
      <button onclick={handleConfirm} class="accent" disabled={busy || !message.trim()}>
        {busy ? (progress ? `记录中...：${progress}` : '记录中...') : '记录 (Ctrl+Enter)'}
      </button>
    </div>
  </div>
</div>

<style>
  :global(.commit-dialog) {
    width: 460px;
  }
  .field textarea {
    resize: none;
  }
  .cat-label {
    display: block;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
    margin-bottom: 4px;
  }
  .cat-row {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 4px;
  }
  .cat-btn {
    font-size: 11.5px;
    padding: 2px 11px;
    border-radius: 999px;
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    transition: all 0.12s var(--ease);
  }
  .cat-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .cat-btn.active {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    font-weight: 600;
  }
  .tag-row {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 4px;
  }
  .tag-btn {
    font-size: 11.5px;
    padding: 2px 11px;
    border-radius: 999px;
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
    transition: all 0.12s var(--ease);
  }
  .tag-btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .tag-btn.active {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    font-weight: 600;
  }
  .custom-btn.active {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .custom-input {
    margin-top: 6px;
    width: 100%;
    padding: 3px 8px;
    font-size: 12px;
    border-radius: var(--radius-sm);
  }
  .row-field {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .row-field label {
    margin-bottom: 0 !important;
  }
  .row-field input[type="checkbox"] {
    width: 14px;
    height: 14px;
    accent-color: var(--accent);
    cursor: pointer;
  }
</style>
