<script lang="ts">
  let {
    text = '处理中...',
  }: {
    text?: string
  } = $props()
</script>

<div class="busy-overlay">
  <div class="busy-box">
    <div class="busy-spinner"></div>
    <div class="busy-text">{text}</div>
  </div>
</div>

<style>
  .busy-overlay {
    position: fixed;
    inset: 0;
    z-index: 300;
    background: color-mix(in srgb, var(--bg) 55%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .busy-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    padding: 24px 40px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.25);
  }
  .busy-spinner {
    width: 28px;
    height: 28px;
    border: 3px solid var(--surface3);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: busy-spin 0.8s linear infinite;
  }
  @keyframes busy-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .busy-text {
    font-size: 13px;
    color: var(--text);
  }
</style>
