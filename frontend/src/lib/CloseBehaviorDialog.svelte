<script lang="ts">
  import { resolveCloseAction } from '../api'

  let {
    onCancel,
  }: {
    onCancel: () => void
  } = $props()

  let busy = $state(false)

  async function pick(closeToTray: boolean) {
    if (busy) return
    busy = true
    try {
      await resolveCloseAction(closeToTray)
    } catch { /* ignore */ }
    onCancel()
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.target as HTMLElement)?.tagName === 'BUTTON') return
    if (e.key === 'Escape') {
      e.preventDefault()
      onCancel()
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="dialog-overlay" onclick={onCancel} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog close-ask-dialog" onclick={(e: MouseEvent) => e.stopPropagation()} role="dialog" tabindex="-1">
    <div class="confirm-head">
      <span class="confirm-icon">
        <svg viewBox="0 0 16 16" width="18" height="18" fill="currentColor"><path d="M8 1.5a6.5 6.5 0 100 13 6.5 6.5 0 000-13zm0 1.5a5 5 0 110 10 5 5 0 010-10z"/><path d="M7.2 4.6h1.6v4.4H7.2zM7.2 10.4h1.6v1.2H7.2z"/></svg>
      </span>
      <h3>关闭窗口时希望怎样？</h3>
    </div>
    <div class="confirm-msg">以后关闭窗口将按此选择执行，可随时在设置中修改；通知区域图标左键打开、右键可退出。</div>
    <div class="actions">
      <button onclick={onCancel}>取消</button>
      <button onclick={() => pick(false)} disabled={busy}>退出应用</button>
      <button class="accent" onclick={() => pick(true)} disabled={busy}>后台驻留</button>
    </div>
  </div>
</div>

<style>
  :global(.close-ask-dialog) {
    width: 420px;
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
