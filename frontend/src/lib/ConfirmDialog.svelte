<script lang="ts">
  let {
    title,
    message,
    okLabel = '确认',
    cancelLabel = '取消',
    onConfirm,
    onCancel,
  }: {
    title: string
    message: string
    okLabel?: string
    cancelLabel?: string
    onConfirm: () => void
    onCancel: () => void
  } = $props()

  function handleKeydown(e: KeyboardEvent) {
    if ((e.target as HTMLElement)?.tagName === 'BUTTON') return
    if (e.key === 'Enter') {
      e.preventDefault()
      onConfirm()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      onCancel()
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="dialog-overlay" onclick={onCancel} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog confirm-dialog" onclick={(e: MouseEvent) => e.stopPropagation()} role="dialog" tabindex="-1">
    <div class="confirm-head">
      <span class="confirm-icon">
        <svg viewBox="0 0 16 16" width="18" height="18" fill="currentColor"><path d="M8 1.5a6.5 6.5 0 100 13 6.5 6.5 0 000-13zm0 1.5a5 5 0 110 10 5 5 0 010-10z"/><path d="M7.2 4.6h1.6v4.4H7.2zM7.2 10.4h1.6v1.2H7.2z"/></svg>
      </span>
      <h3>{title}</h3>
    </div>
    <div class="confirm-msg">{message}</div>
    <div class="actions">
      <button onclick={onCancel}>{cancelLabel}</button>
      <button onclick={onConfirm} class="accent">{okLabel}</button>
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
    gap: 10px;
    margin-bottom: 10px;
  }
  .confirm-icon {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-soft);
    color: var(--accent);
    flex-shrink: 0;
  }
  .confirm-msg {
    font-size: 13px;
    line-height: 1.7;
    color: var(--text);
    white-space: pre-line;
    margin-bottom: 18px;
  }
</style>
