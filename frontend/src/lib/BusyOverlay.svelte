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
    background: color-mix(in srgb, var(--bg) 50%, transparent);
    backdrop-filter: blur(3px);
    -webkit-backdrop-filter: blur(3px);
    display: flex;
    align-items: center;
    justify-content: center;
    animation: fade-in 0.12s var(--ease);
  }
  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  .busy-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 20px 36px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
  }
  .busy-spinner {
    width: 24px;
    height: 24px;
    border: 2.5px solid var(--surface3);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: busy-spin 0.7s linear infinite;
  }
  @keyframes busy-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .busy-text {
    font-size: 12.5px;
    color: var(--text);
  }
</style>
