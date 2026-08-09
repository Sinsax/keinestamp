<script lang="ts">
  let {
    onConfirm,
    onCancel,
  }: {
    onConfirm: () => void
    onCancel: () => void
  } = $props()

  let countdown = $state(5)
  let ready = $derived(countdown <= 0)
  let timer: ReturnType<typeof setInterval> | null = null

  $effect(() => {
    countdown = 5
    timer = setInterval(() => {
      if (countdown > 0) {
        countdown--
      } else if (timer) {
        clearInterval(timer)
        timer = null
      }
    }, 1000)
    return () => {
      if (timer) clearInterval(timer)
    }
  })
</script>

<div class="dialog-overlay" onclick={onCancel} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog rollback-dialog" onclick={(e: MouseEvent) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>恢复回退前的状态</h3>
    <p class="restore-warn">
      文件将恢复到回退前的状态；回退期间的修改会合并带回，冲突文件需逐文件选择保留版本。
    </p>
    <div class="actions">
      <button onclick={onCancel}>取消</button>
      <button class="accent" disabled={!ready} onclick={onConfirm}>
        {ready ? '确认恢复' : `确认恢复 (${countdown}s)`}
      </button>
    </div>
  </div>
</div>

<style>
  .restore-warn {
    font-size: 12px;
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
    background: var(--accent-soft);
    border-radius: 8px;
    padding: 8px 10px;
    margin-bottom: 14px;
  }
</style>
