<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import logo from '../assets/logo.png'

  let win: any = undefined
  try {
    win = getCurrentWindow()
  } catch (e) {
    console.error('TitleBar: getCurrentWindow failed', e)
  }

  function doMinimize() {
    if (!win) return
    win.minimize().catch((e: any) => console.error('minimize err:', e))
  }

  function doMaximize() {
    if (!win) return
    win.isMaximized().then((max: boolean) => {
      if (max) {
        win.unmaximize().catch((e: any) => console.error('unmaximize err:', e))
      } else {
        win.maximize().catch((e: any) => console.error('maximize err:', e))
      }
    }).catch((e: any) => console.error('isMaximized err:', e))
  }

  function doClose() {
    if (!win) return
    win.close().catch((e: any) => console.error('close err:', e))
  }
</script>

<div class="titlebar">
  <div class="tb-brand">
    <img class="tb-icon" src={logo} alt="Keinestamp logo" />
    <span class="tb-title">Keinestamp</span>
    </div>
    <div class="tb-subtitle">Lore Progress Board</div>
  <div class="tb-spacer"></div>
  <div class="tb-controls">
    <button class="tb-btn" onclick={doMinimize} title="Minimize">
      <svg viewBox="0 0 12 12" width="10" height="10" fill="currentColor"><path d="M2 6h8v1H2z"/></svg>
    </button>
    <button class="tb-btn" onclick={doMaximize} title="Maximize">
      <svg viewBox="0 0 12 12" width="10" height="10" fill="currentColor"><path d="M2 2h8v8H2zm1 1v6h6V3z"/></svg>
    </button>
    <button class="tb-btn tb-close" onclick={doClose} title="Close">
      <svg viewBox="0 0 12 12" width="10" height="10" fill="currentColor"><path d="M2.646 2.646a.5.5 0 0 1 .708 0L6 5.293l2.646-2.647a.5.5 0 0 1 .708.708L6.707 6l2.647 2.646a.5.5 0 0 1-.708.708L6 6.707l-2.646 2.647a.5.5 0 0 1-.708-.708L5.293 6 2.646 3.354a.5.5 0 0 1 0-.708z"/></svg>
    </button>
  </div>
</div>

<style>
  .titlebar {
    display: flex;
    align-items: center;
    height: 32px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    padding: 0 10px;
    flex-shrink: 0;
    -webkit-app-region: drag;
    user-select: none;
  }
  .tb-brand {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 100%;
  }
  .tb-icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }
  .tb-title {
    font-size: 12.5px;
    font-weight: 650;
    letter-spacing: 0.2px;
    white-space: nowrap;
  }
  .tb-subtitle {
    font-size: 10px;
    color: var(--text-dim);
    margin-left: 8px;
    white-space: nowrap;
    opacity: 0.8;
  }
  .tb-spacer {
    flex: 1;
    height: 100%;
  }
  .tb-controls {
    display: flex;
    gap: 1px;
    margin-right: -4px;
    height: 100%;
    align-items: center;
    flex-shrink: 0;
  }
  .tb-btn {
    background: none;
    border: none;
    color: var(--text-dim);
    cursor: pointer;
    padding: 4px 9px;
    border-radius: var(--radius-sm);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 24px;
    -webkit-app-region: no-drag;
    transition: background 0.1s, color 0.1s;
  }
  .tb-btn:hover {
    background: var(--surface2);
    color: var(--text);
  }
  .tb-close:hover {
    background: var(--error);
    color: #fff;
  }
</style>
