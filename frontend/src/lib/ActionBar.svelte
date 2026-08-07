<script lang="ts">
  import type { ProjectType } from '../types'

  let {
    projectType,
    onRecord,
    onExport,
    onRevert,
    onRestore,
    onDeploy,
    connected = false,
    deployAhead = false,
    deployTip = '部署到服务器',
    canRestore = false,
  }: {
    projectType: ProjectType | null
    onRecord: () => void
    onExport: () => void
    onRevert: () => void
    onRestore: () => void
    onDeploy: () => void
    connected?: boolean
    deployAhead?: boolean
    deployTip?: string
    canRestore?: boolean
  } = $props()

  const initialized = $derived(projectType === 'initialized')
</script>

<div class="action-bar">
  <button class="record-btn" onclick={onRecord} disabled={!initialized}>
    <span class="ab-label">记录</span>
    <span class="ab-hint">record</span>
  </button>

  <button onclick={onExport} disabled={!initialized}>
    <span class="ab-label">导出</span>
    <span class="ab-hint">export</span>
  </button>

  <button onclick={onRevert} disabled={!initialized}>
    <span class="ab-label">撤回</span>
    <span class="ab-hint">revert</span>
  </button>

  <button class="restore-btn" class:enabled={canRestore} onclick={onRestore} disabled={!initialized || !canRestore} title={canRestore ? '恢复上次撤回的记录' : '没有可恢复的撤回'}>
    <span class="ab-label">恢复</span>
    <span class="ab-hint">redo</span>
  </button>

  <div class="spacer"></div>

  <button class="deploy-btn" class:ahead={deployAhead} onclick={onDeploy} disabled={!initialized || !connected} title={deployTip}>
    <span class="ab-label">部署</span>
    <span class="ab-hint">deploy</span>
  </button>
</div>

<style>
  .action-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    height: var(--action-h);
    padding: 0 10px;
    background: var(--surface);
    border-top: 1px solid var(--border);
  }
  .action-bar button {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
    font-size: 11px;
    padding: 4px 16px;
    border-radius: 8px;
    cursor: pointer;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text);
    transition: background 0.15s var(--ease), border-color 0.15s var(--ease),
      box-shadow 0.15s var(--ease);
  }
  .action-bar button:hover:not(:disabled) {
    background: var(--surface2);
    border-color: var(--surface3);
  }
  .action-bar button:disabled {
    opacity: 0.35;
    cursor: default;
    pointer-events: none;
  }
  .ab-label {
    font-size: 13px;
    font-weight: 600;
    line-height: 1.2;
  }
  .ab-hint {
    font-size: 9px;
    color: var(--text-dim);
    line-height: 1.1;
  }
  .record-btn {
    background: var(--accent) !important;
    border-color: var(--accent) !important;
    color: #fff !important;
  }
  .record-btn .ab-hint {
    color: rgba(255, 255, 255, 0.75);
  }
  .record-btn:hover:not(:disabled) {
    background: var(--accent-hover) !important;
    border-color: var(--accent-hover) !important;
    box-shadow: 0 2px 8px rgba(217, 71, 71, 0.35);
  }
  .deploy-btn {
    border-color: var(--success) !important;
    color: var(--success) !important;
    background: transparent !important;
  }
  .deploy-btn:hover:not(:disabled) {
    background: var(--success-soft) !important;
    border-color: var(--success) !important;
    color: var(--success) !important;
  }
  .deploy-btn .ab-hint {
    color: var(--success);
    opacity: 0.75;
  }
  .deploy-btn.ahead {
    border-color: var(--error) !important;
    color: var(--error) !important;
  }
  .deploy-btn.ahead .ab-hint {
    color: var(--error);
    opacity: 0.75;
  }
  .deploy-btn.ahead:hover:not(:disabled) {
    background: var(--accent-soft) !important;
    border-color: var(--error) !important;
    color: var(--error) !important;
  }
  .spacer { flex: 1; }
  .restore-btn.enabled {
    border-color: var(--accent) !important;
    color: var(--accent) !important;
  }
  .restore-btn.enabled:hover:not(:disabled) {
    background: var(--accent-soft) !important;
    border-color: var(--accent) !important;
  }
</style>