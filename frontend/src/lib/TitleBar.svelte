<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'

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
    <svg class="tb-icon" viewBox="0 0 100 100" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M28 22 C44 14, 56 14, 72 22 C84 34, 84 66, 72 78 C56 86, 44 86, 28 78 C16 66, 16 34, 28 22Z" fill="#D94747"/>
      <path d="M33 28 C22 40, 22 60, 33 72" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
      <path d="M44 24 C32 40, 32 60, 44 76" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
      <path d="M56 24 C44 40, 44 60, 56 76" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
      <path d="M67 28 C56 40, 56 60, 67 72" stroke="#B03030" stroke-width="2.5" fill="none" opacity="0.35"/>
      <circle cx="40" cy="40" r="3" fill="#FFE3E3"/>
      <circle cx="44" cy="55" r="3.5" fill="#9FC3E3"/>
      <circle cx="50" cy="70" r="3" fill="#FFE3E3"/>
      <circle cx="52" cy="38" r="3.5" fill="#9FC3E3"/>
      <circle cx="58" cy="53" r="3" fill="#FFE3E3"/>
      <circle cx="62" cy="68" r="3.5" fill="#9FC3E3"/>
      <path d="M46 16 C48 10, 52 6, 58 8" stroke="#E8B4B4" stroke-width="3" fill="none" stroke-linecap="round"/>
      <path d="M58 8 C62 6, 64 8, 66 7" stroke="#E8B4B4" stroke-width="2.5" fill="none" stroke-linecap="round"/>
    </svg>
    <span class="tb-title">Keinestamp</span>
    </div>
    <div class="tb-subtitle">Blender Git Progress Board</div>
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
    height: 34px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
    padding: 0 8px;
    flex-shrink: 0;
    -webkit-app-region: drag;
    user-select: none;
  }
  .tb-brand {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 100%;
  }
  .tb-icon {
    width: 18px;
    height: 18px;
    flex-shrink: 0;
  }
  .tb-title {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.3px;
    white-space: nowrap;
  }
  .tb-subtitle {
    font-size: 10px;
    color: var(--text-dim);
    margin-left: 10px;
    white-space: nowrap;
  }
  .tb-spacer {
    flex: 1;
    height: 100%;
  }
  .tb-controls {
    display: flex;
    gap: 2px;
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
    padding: 4px 8px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 26px;
    -webkit-app-region: no-drag;
    transition: background 0.1s, color 0.1s;
  }
  .tb-btn:hover {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .tb-close:hover {
    background: var(--error);
    color: #fff;
  }
</style>
