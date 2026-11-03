<script lang="ts">
  import { onMount } from 'svelte'
    import * as api from '../api'
  import { t } from './i18n/index.svelte'
  import { open } from '@tauri-apps/plugin-dialog'
  import InputDialog from './InputDialog.svelte'

  let {
    loreServerUrl,
    recentPaths = [],
    enabled = true,
    embedded = false,
    onOpenProject = (_path: string) => {},
    onGoSettings = () => {},
  }: {
    loreServerUrl: string
    recentPaths?: string[]
    enabled?: boolean
    embedded?: boolean
    onOpenProject?: (path: string) => void
    onGoSettings?: () => void
  } = $props()

  let serverConn = $state<'unknown' | 'ok' | 'fail'>('unknown')
  let serverMsg = $state('')
  let serverRepos = $state<[string, string][]>([])
  let serverLoading = $state(false)
  let selectedRepos = $state<Set<string>>(new Set())
  let downloading = $state(false)
  let downloadResults = $state<{ name: string; ok: boolean; msg: string }[]>([])
  let deleting = $state(false)
  let deleteTarget = $state<[string, string] | null>(null)

  // 已下载映射：recent 项目目录名 == 仓库名 → 完整本地路径（跳转按钮用）
  const localPaths = $derived(
    new Map(
      recentPaths.map((p) => {
        const clean = p.replace(/[/\\]+$/, '')
        const name = clean.split(/[/\\]/).pop() ?? p
        return [name, clean]
      })
    )
  )

  // 进入页面自动刷新（服务开关关闭时不拉取）
  onMount(() => {
    if (enabled) refreshServerRepos()
  })

  async function refreshServerRepos() {
    if (!loreServerUrl) {
      serverConn = 'fail'
      serverMsg = t('gui.remote.noUrl')
      serverRepos = []
      return
    }
    serverLoading = true
    serverConn = 'unknown'
    serverMsg = ''
    try {
      const repos = await api.listServerRepos(loreServerUrl)
      serverConn = 'ok'
      serverRepos = repos
      serverMsg = repos.length > 0 ? '' : t('gui.remote.noRepos')
    } catch (e: any) {
      serverConn = 'fail'
      serverMsg = String(e)
      serverRepos = []
      console.error('list server repos failed', e)
    }
    serverLoading = false
  }

  function toggleRepo(name: string) {
    const next = new Set(selectedRepos)
    if (next.has(name)) next.delete(name)
    else next.add(name)
    selectedRepos = next
  }

  async function downloadSelected() {
    if (!loreServerUrl || selectedRepos.size === 0) return
    const picked = await open({ directory: true, multiple: false, title: t('gui.remote.pickDir') })
    if (!picked) return
    const normParent = picked.replace(/^\\\\\?\\/, '')
    downloading = true
    downloadResults = []
    let firstCloned: string | null = null
    for (const [name] of serverRepos) {
      if (!selectedRepos.has(name)) continue
      try {
        const repoPath = await api.cloneServerRepo(loreServerUrl, name, normParent)
        downloadResults = [...downloadResults, { name, ok: true, msg: repoPath }]
        if (!firstCloned) firstCloned = repoPath
      } catch (e: any) {
        downloadResults = [...downloadResults, { name, ok: false, msg: String(e) }]
        console.error(`clone server repo failed: ${name}`, e)
      }
    }
    downloading = false
    selectedRepos = new Set()
    if (firstCloned) onOpenProject(firstCloned)
  }

  // 操作反馈自动消失：serverMsg 原先只在「列表为空 / 连接失败」分支渲染，
  // 仓库列表非空时删除成功/失败都看不到（2026-10 审查 P1）
  $effect(() => {
    if (!serverMsg) return
    const timer = setTimeout(() => { serverMsg = '' }, 8000)
    return () => clearTimeout(timer)
  })

  // 删除远端仓库：弹窗输入 DELETE 确认（删服务器仓库 = 备份数据丢失）
  async function performDelete(name: string) {
    if (!loreServerUrl) return
    deleting = true
    try {
      await api.deleteServerRepo(loreServerUrl, name)
      serverRepos = serverRepos.filter(([n]) => n !== name)
      selectedRepos.delete(name)
      serverMsg = t('gui.remote.deleted', { name })
    } catch (e: any) {
      serverMsg = t('gui.remote.deleteFailed', { error: e })
      console.error('delete server repo failed', e)
    }
    deleting = false
    deleteTarget = null
  }
</script>

<div class="remote" class:embedded>
  {#if !enabled}
    <div class="remote-empty">{t('gui.remote.disabled')}</div>
  {:else}
  <div class="remote-head">
    <div>
      <div class="remote-title">{t('gui.remote.title')}</div>
      <div class="remote-sub">
        {loreServerUrl || t('gui.remote.unset')}
        {#if serverConn === 'ok'}
          <span class="conn-badge ok">{t('gui.remote.connected')}</span>
        {:else if serverConn === 'fail'}
          <span class="conn-badge fail">{t('gui.remote.disconnected')}</span>
        {/if}
        {#if serverRepos.length > 0}
          <span class="conn-badge count">{t('gui.remote.count', { n: serverRepos.length })}</span>
        {/if}
      </div>
    </div>
    <div class="remote-actions">
      <button class="rl-btn" onclick={refreshServerRepos} disabled={serverLoading || downloading}>
        {serverLoading ? t('common.loading') : t('common.refresh')}
      </button>
      {#if serverRepos.length > 0}
        <button
          class="rl-btn primary"
          onclick={downloadSelected}
          disabled={selectedRepos.size === 0 || downloading}
        >
          {downloading ? t('gui.remote.downloading') : t('gui.remote.downloadTo', { n: selectedRepos.size })}
        </button>
      {/if}
    </div>
  </div>

  {#if serverMsg && serverRepos.length > 0}
    <div class="remote-note" role="status">
      <span>{serverMsg}</span>
      <button class="rl-link" onclick={() => (serverMsg = '')} title={t('gui.remote.closeHint')}>×</button>
    </div>
  {/if}

  {#if serverConn === 'fail'}
    {serverMsg || t('gui.remote.unreachable')}
    {#if !loreServerUrl}
      <div class="remote-empty small">
        <button class="rl-link" onclick={onGoSettings}>{t('gui.remote.goSettings')}</button>
      </div>
    {/if}
  {:else if serverLoading}
    <div class="remote-empty">{t('gui.remote.loadingRepos')}</div>
  {:else if serverRepos.length === 0}
    {serverMsg || t('gui.remote.emptyHint')}
  {:else}
    <div class="repo-list">
      {#each serverRepos as [name, id] (id)}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div
          class="repo-row"
          class:selected={selectedRepos.has(name)}
          onclick={() => toggleRepo(name)}
        >
          <span class="repo-check">{selectedRepos.has(name) ? '☑' : '☐'}</span>
          <span class="repo-name">{name}</span>
          {#if localPaths.has(name)}
            <button
              class="open-btn"
              title={t('gui.remote.existingTitle')}
              onclick={(e) => { e.stopPropagation(); const p = localPaths.get(name); if (p) onOpenProject(p) }}
            >
              {t('gui.remote.openExisting')}
            </button>
          {/if}
          <span class="repo-id">{id.slice(0, 8)}</span>
          <button
            class="del-btn"
            disabled={deleting}
            onclick={(e) => { e.stopPropagation(); deleteTarget = [name, id] }}
            title={t('gui.remote.deleteServerTitle')}
          >
            {deleting ? t('gui.remote.deleting') : t('common.delete')}
          </button>
        </div>
      {/each}
    </div>
  {/if}

  {#if downloadResults.length > 0}
    <div class="dl-results">
      {#each downloadResults as r (r.name)}
        <div class="dl-result" class:ok={r.ok}>
          <span class="dl-icon">{r.ok ? '✓' : '✕'}</span>
          <span class="dl-name">{r.name}</span>
          <span class="dl-msg">{r.msg}</span>
        </div>
      {/each}
    </div>
  {/if}
  {/if}
</div>

{#if deleteTarget}
  <InputDialog
    title={t('gui.remote.deleteAriaTitle')}
    label={t('gui.remote.deleteConfirm', { name: deleteTarget[0] })}
    mustEqual="DELETE"
    okLabel={t('common.delete')}
    danger
    uppercase
    onConfirm={() => { const target = deleteTarget; if (target) performDelete(target[0]) }}
    onCancel={() => (deleteTarget = null)}
  />
{/if}

<style>
  .remote-note {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin: 0 0 8px;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--surface2, #f1f3f5);
    color: var(--text-2, #506070);
    font-size: 12px;
    word-break: break-all;
  }
  .remote {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 20px 24px;
    max-width: 860px;
    margin: 0 auto;
    overflow-y: auto;
    height: 100%;
  }
  .remote.embedded {
    padding: 0;
    max-width: none;
    height: auto;
    overflow: visible;
  }
  .open-btn {
    background: var(--accent-soft);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
    font-size: 11px;
    padding: 1px 9px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .open-btn:hover {
    background: var(--accent);
    color: #fff;
  }
  .remote-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .remote-title {
    font-size: 13px;
    font-weight: 650;
    color: var(--text);
  }
  .remote-sub {
    font-size: 10.5px;
    color: var(--text-dim);
    font-family: var(--mono);
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 2px;
  }
  .remote-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .rl-btn {
    background: none;
    border: 1px solid var(--border-strong);
    color: var(--text-dim);
    font-size: 11px;
    padding: 2px 10px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color 0.12s, border-color 0.12s, background 0.12s;
  }
  .rl-btn:hover:not(:disabled) {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .rl-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .rl-btn.primary {
    border-color: var(--accent);
    color: var(--accent);
  }
  .rl-link {
    background: none;
    border: none;
    color: var(--accent);
    font-size: 12px;
    cursor: pointer;
    padding: 0;
  }
  .conn-badge {
    font-size: 10px;
    padding: 1px 8px;
    border-radius: 999px;
    font-family: inherit;
    font-weight: 600;
    line-height: 1.7;
  }
  .conn-badge.ok {
    color: var(--success);
    background: color-mix(in srgb, var(--success) 14%, transparent);
  }
  .conn-badge.fail {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 12%, transparent);
  }
  .conn-badge.count {
    color: var(--text-dim);
    background: var(--surface2);
  }
  .remote-empty {
    font-size: 12px;
    color: var(--text-dim);
    padding: 16px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius);
    text-align: center;
  }
  .remote-empty.small {
    padding: 6px;
    border: none;
  }
  .repo-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .repo-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 12px;
    transition: border-color 0.12s, background 0.12s;
  }
  .repo-row:hover {
    border-color: var(--accent);
  }
  .repo-row.selected {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 7%, transparent);
  }
  .repo-check {
    color: var(--accent);
  }
  .repo-name {
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .repo-id {
    margin-left: auto;
    font-size: 10px;
    color: var(--text-dim);
    font-family: var(--mono);
  }
  .del-btn {
    background: none;
    border: 1px solid color-mix(in srgb, var(--error) 40%, transparent);
    color: var(--error);
    font-size: 10px;
    padding: 1px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    flex-shrink: 0;
    transition: background 0.12s;
  }
  .del-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--error) 12%, transparent);
  }
  .del-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .dl-results {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .dl-result {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    background: var(--surface2);
  }
  .dl-result.ok .dl-icon { color: var(--success); }
  .dl-result:not(.ok) { color: var(--error); }
  .dl-name { font-weight: 600; }
  .dl-msg {
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>

