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
    padding: 0 12px;
    background: var(--surface);
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-dim);
  }
  .status-left { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status-right { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .conn-indicator { display: inline-flex; align-items: center; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: #666; transition: background 0.3s; }
  .connected .dot { background: var(--success); }
  .token-input { width: 120px; font-size: 11px; padding: 1px 6px; border: 1px solid var(--border); border-radius: 4px; background: var(--bg); color: var(--text); }
  .token-display { cursor: pointer; letter-spacing: 2px; font-size: 13px; color: var(--text-dim); }
  .token-display:hover { color: var(--text); }
  .token-toggle { background: none; border: none; cursor: pointer; color: var(--text-dim); padding: 2px; display: inline-flex; align-items: center; line-height: 1; }
  .token-toggle:hover { color: var(--text); }
</style>
