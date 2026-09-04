<script lang="ts">
  let {
    text = '',
    connected = false,
  }: {
    text?: string
    connected?: boolean
  } = $props()
</script>

<div class="status-bar">
  <span class="status-left">{text || 'Ready'}</span>
  <span class="status-right">
    <span class="conn-indicator" class:connected title={connected ? '已连接' : '未连接'}>
      <span class="dot"></span>
    </span>
  </span>
</div>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: var(--status-h);
    padding: 0 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-size: 10.5px;
    color: var(--text-dim);
  }
  .status-left { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status-right { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .conn-indicator { display: inline-flex; align-items: center; }
  .dot {
    width: 7px; height: 7px;
    border-radius: 50%;
    background: var(--text-dim);
    opacity: 0.5;
    transition: background 0.2s var(--ease), opacity 0.2s var(--ease), box-shadow 0.2s var(--ease);
  }
  .connected .dot {
    background: var(--success);
    opacity: 1;
    box-shadow: 0 0 0 2.5px color-mix(in srgb, var(--success) 18%, transparent);
  }
</style>
