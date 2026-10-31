<script lang="ts">
  let {
    title,
    message,
    okLabel = '确认',
    cancelLabel = '取消',
    danger = false,
    onConfirm,
    onCancel,
  }: {
    title: string
    message: string
    okLabel?: string
    cancelLabel?: string
    // 危险操作确认（如覆盖文件）：确定按钮标红
    danger?: boolean
    onConfirm: () => void
    onCancel: () => void
  } = $props()

  // Enter 只在本对话框「没有输入焦点」时才代表确认：
  // 否则在 CommitDialog 的说明框里换行会顺手把破坏性操作执行掉（2026-10 审查 P1）
  function isTypingTarget(target: EventTarget | null): boolean {
    const el = target as HTMLElement | null
    if (!el) return false
    const tag = el.tagName
    return (
      tag === 'INPUT' ||
      tag === 'TEXTAREA' ||
      tag === 'SELECT' ||
      el.isContentEditable === true
    )
  }

  // 挂载后把焦点移进对话框：否则用户刚点过底层按钮，Enter 的事件目标仍是那个按钮，
  // 会绕过确认直接把原操作重跑一遍（复核 N-10）
  let dialogEl: HTMLDivElement | undefined = $state()

  $effect(() => {
    dialogEl?.focus()
  })

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault()
      onCancel()
      return
    }
    if (e.key !== 'Enter') return
    // 按钮上的 Enter 交给按钮自己触发 click，否则会重复确认
    if ((e.target as HTMLElement)?.tagName === 'BUTTON') return
    // 正在输入框/文本域里打字（含中文输入法组合中）→ 不当作确认
    if (isTypingTarget(e.target) || e.isComposing) return
    e.preventDefault()
    onConfirm()
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="dialog-overlay" onclick={onCancel} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="dialog confirm-dialog"
    bind:this={dialogEl}
    onclick={(e: MouseEvent) => e.stopPropagation()}
    role="dialog"
    tabindex="-1"
  >
    <div class="confirm-head">
      <span class="confirm-icon">
        <svg viewBox="0 0 16 16" width="18" height="18" fill="currentColor"><path d="M8 1.5a6.5 6.5 0 100 13 6.5 6.5 0 000-13zm0 1.5a5 5 0 110 10 5 5 0 010-10z"/><path d="M7.2 4.6h1.6v4.4H7.2zM7.2 10.4h1.6v1.2H7.2z"/></svg>
      </span>
      <h3>{title}</h3>
    </div>
    <div class="confirm-msg">{message}</div>
    <div class="actions">
      <button onclick={onCancel}>{cancelLabel}</button>
      <button onclick={onConfirm} class:accent={!danger} class:danger={danger}>{okLabel}</button>
    </div>
  </div>
</div>

<style>
  :global(.confirm-dialog) {
    width: 400px;
  }
  .confirm-head {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 8px;
  }
  .confirm-icon {
    width: 26px;
    height: 26px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-soft);
    color: var(--accent);
    flex-shrink: 0;
  }
  .confirm-msg {
    font-size: 12.5px;
    line-height: 1.65;
    color: var(--text);
    white-space: pre-line;
    margin-bottom: 16px;
  }
</style>
