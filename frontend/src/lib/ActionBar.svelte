<script lang="ts">
  import type { ProjectType } from '../types'
  import { t } from './i18n/index.svelte'

  let {
    projectType,
    onRecord,
    onRestore,
    onDeploy,
    onBackup = () => {},
    connected = false,
    deployAhead = false,
    deployTip = '',
    restoreEnabled = false,
    restoreTitle = '',
    busy = false,
    loreOk = true,
  }: {
    projectType: ProjectType | null
    onRecord: () => void
    onRestore: () => void
    onDeploy: () => void
    onBackup?: () => void
    connected?: boolean
    deployAhead?: boolean
    deployTip?: string
    restoreEnabled?: boolean
    restoreTitle?: string
    busy?: boolean
    /** Lore 前置依赖（编译期捆绑）检测通过后才可用 */
    loreOk?: boolean
  } = $props()

  const initialized = $derived(projectType === 'initialized')
  const loreReady = $derived(loreOk && initialized)
  const loreTitle = $derived(t('gui.actionbar.loreMissing'))
  // 文案在组件内解析（props 只作为覆盖项），切换语言时自动跟随
  const deployTitle = $derived(deployTip || t('gui.actionbar.deployTip'))
  const restoreTip = $derived(restoreTitle || t('gui.actionbar.restoreNone'))
</script>

<div class="action-bar">
  <button class="record-btn" onclick={onRecord} disabled={!loreReady || busy} title={loreOk ? '' : loreTitle}>
    <span class="ab-label">{t('gui.actionbar.record')}</span>
    <span class="ab-hint">record</span>
  </button>

  <button class="restore-btn" class:enabled={restoreEnabled} onclick={onRestore} disabled={!loreReady || !restoreEnabled || busy} title={loreOk ? restoreTip : loreTitle}>
    <span class="ab-label">{t('gui.actionbar.restore')}</span>
    <span class="ab-hint">redo</span>
  </button>

  <div class="spacer"></div>

  <button class="backup-btn" onclick={onBackup} disabled={!loreReady || busy} title={loreOk ? t('gui.actionbar.backupTip') : loreTitle}>
    <span class="ab-label">{t('gui.actionbar.backup')}</span>
    <span class="ab-hint">backup</span>
  </button>

  <button class="deploy-btn" class:ahead={deployAhead} onclick={onDeploy} disabled={!loreReady || !connected || busy} title={loreOk ? deployTitle : loreTitle}>
    <span class="ab-label">{t('gui.actionbar.deploy')}</span>
    <span class="ab-hint">deploy</span>
  </button>
</div>

<style>
  .action-bar {
    display: flex;
    align-items: center;
    gap: 5px;
    height: var(--action-h);
    padding: 0 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-panel);
  }
  .action-bar button {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0;
    font-size: 11px;
    padding: 2px 14px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text);
    transition: background 0.12s var(--ease), border-color 0.12s var(--ease),
      color 0.12s var(--ease), box-shadow 0.12s var(--ease);
  }
  .action-bar button:hover:not(:disabled) {
    background: var(--surface2);
    border-color: transparent;
  }
  .action-bar button:disabled {
    opacity: 0.35;
    cursor: default;
    pointer-events: none;
  }
  .ab-label {
    font-size: 12px;
    font-weight: 600;
    line-height: 1.25;
  }
  .ab-hint {
    font-size: 8.5px;
    color: var(--text-dim);
    line-height: 1.15;
    letter-spacing: 0.3px;
    text-transform: uppercase;
  }
  .record-btn {
    flex-direction: row;
    gap: 6px;
    padding: 4px 18px;
    border-radius: 999px;
    background: var(--accent) !important;
    border-color: var(--accent) !important;
    color: #fff !important;
    box-shadow: var(--accent-shadow);
  }
  .record-btn .ab-label {
    font-size: 12.5px;
  }
  .record-btn .ab-hint {
    color: rgba(255, 255, 255, 0.72);
  }
  .record-btn:hover:not(:disabled) {
    background: var(--accent-hover) !important;
    border-color: var(--accent-hover) !important;
  }
  .deploy-btn {
    color: var(--success) !important;
    background: transparent !important;
  }
  .backup-btn {
    color: var(--tint) !important;
    background: transparent !important;
    margin-right: 8px; /* 与部署保持间隔（同属服务器功能） */
  }
  .backup-btn:hover:not(:disabled) {
    background: var(--tint-soft) !important;
    color: var(--tint) !important;
  }
  .backup-btn .ab-hint {
    color: var(--tint);
    opacity: 0.75;
  }
  .deploy-btn:hover:not(:disabled) {
    background: var(--success-soft) !important;
    color: var(--success) !important;
  }
  .deploy-btn .ab-hint {
    color: var(--success);
    opacity: 0.75;
  }
  .deploy-btn.ahead {
    color: var(--error) !important;
  }
  .deploy-btn.ahead .ab-hint {
    color: var(--error);
    opacity: 0.75;
  }
  .deploy-btn.ahead:hover:not(:disabled) {
    background: color-mix(in srgb, var(--error) 9%, transparent) !important;
    color: var(--error) !important;
  }
  .spacer { flex: 1; }
  .restore-btn.enabled {
    color: var(--accent) !important;
  }
  .restore-btn.enabled:hover:not(:disabled) {
    background: var(--accent-soft) !important;
  }
</style>
