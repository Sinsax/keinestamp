<script lang="ts">
  import { onMount } from 'svelte'
  import * as api from '../api'
  import { open } from '@tauri-apps/plugin-dialog'
  import InputDialog from './InputDialog.svelte'

  let {
    loreServerUrl,
    recentPaths = [],
    onOpenProject = (_path: string) => {},
    onGoSettings = () => {},
  }: {
    loreServerUrl: string
    recentPaths?: string[]
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

  // 已下载标记：recent 项目目录名 == 仓库名
  const localNames = $derived(new Set(recentPaths.map((p) => p.replace(/[/\\]+$/, '').split(/[/\\]/).pop() ?? p)))

  // 进入页面自动刷新
  onMount(() => {
    refreshServerRepos()
  })

  async function refreshServerRepos() {
    if (!loreServerUrl) {
      serverConn = 'fail'
      serverMsg = '未配置 loreserver 地址'
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
      serverMsg = repos.length > 0 ? '' : '服务器上没有仓库'
    } catch (e: any) {
      serverConn = 'fail'
      serverMsg = String(e)
      serverRepos = []
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
    const picked = await open({ directory: true, multiple: false, title: '选择仓库保存目录' })
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
      }
    }
    downloading = false
    selectedRepos = new Set()
    if (firstCloned) onOpenProject(firstCloned)
  }

  // 删除远端仓库：弹窗输入 DELETE 确认（删服务器仓库 = 备份数据丢失）
  async function performDelete(name: string) {
    if (!loreServerUrl) return
    deleting = true
    try {
      await api.deleteServerRepo(loreServerUrl, name)
      serverRepos = serverRepos.filter(([n]) => n !== name)
      selectedRepos.delete(name)
      serverMsg = `已删除仓库 ${name}`
    } catch (e: any) {
      serverMsg = `删除失败: ${e}`
    }
    deleting = false
    deleteTarget = null
  }
</script>

<div class="remote">
  <div class="remote-head">
    <div>
      <div class="remote-title">远端仓库（loreserver）</div>
      <div class="remote-sub">
        {loreServerUrl || '未配置地址'}
        {#if serverConn === 'ok'}
          <span class="conn-badge ok">已连接</span>
        {:else if serverConn === 'fail'}
          <span class="conn-badge fail">未连接</span>
        {/if}
        {#if serverRepos.length > 0}
          <span class="conn-badge count">共 {serverRepos.length} 个仓库</span>
        {/if}
      </div>
    </div>
    <div class="remote-actions">
      <button class="rl-btn" onclick={refreshServerRepos} disabled={serverLoading || downloading}>
        {serverLoading ? '加载中...' : '刷新'}
      </button>
      {#if serverRepos.length > 0}
        <button
          class="rl-btn primary"
          onclick={downloadSelected}
          disabled={selectedRepos.size === 0 || downloading}
        >
          {downloading ? '下载中...' : `下载到本地（${selectedRepos.size}）`}
        </button>
      {/if}
    </div>
  </div>

  {#if serverConn === 'fail'}
    <div class="remote-empty">{serverMsg || 'loreserver 不可达'}</div>
    {#if !loreServerUrl}
      <div class="remote-empty small">
        <button class="rl-link" onclick={onGoSettings}>去设置配置 loreserver 地址</button>
      </div>
    {/if}
  {:else if serverLoading}
    <div class="remote-empty">加载仓库列表...</div>
  {:else if serverRepos.length === 0}
    <div class="remote-empty">{serverMsg || '服务器上没有仓库 — 先在本机备份一个项目，或点击「连接」'}</div>
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
          {#if localNames.has(name)}
            <span class="dl-badge" title="本地常用仓库中已有同名项目">已下载</span>
          {/if}
          <span class="repo-id">{id.slice(0, 8)}</span>
          <button
            class="del-btn"
            disabled={deleting}
            onclick={(e) => { e.stopPropagation(); deleteTarget = [name, id] }}
            title="删除服务器仓库（备份数据将丢失）"
          >
            {deleting ? '删除中...' : '删除'}
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
</div>

{#if deleteTarget}
  <InputDialog
    title="删除远端仓库"
    label={`输入 DELETE 确认删除「${deleteTarget[0]}」（备份数据将丢失，不可恢复）`}
    mustEqual="DELETE"
    okLabel="删除"
    danger
    uppercase
    onConfirm={() => { const t = deleteTarget; if (t) performDelete(t[0]) }}
    onCancel={() => (deleteTarget = null)}
  />
{/if}

<style>
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
  .remote-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .remote-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }
  .remote-sub {
    font-size: 11px;
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
    gap: 8px;
  }
  .rl-btn {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 11px;
    padding: 4px 12px;
    border-radius: 6px;
    cursor: pointer;
    transition: color 0.12s, border-color 0.12s;
  }
  .rl-btn:hover:not(:disabled) {
    color: var(--accent);
    border-color: var(--accent);
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
    padding: 2px 8px;
    border-radius: 999px;
    font-family: var(--font-sans, sans-serif);
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
    padding: 18px;
    border: 1px dashed var(--border);
    border-radius: 10px;
    text-align: center;
  }
  .remote-empty.small {
    padding: 6px;
    border: none;
  }
  .repo-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .repo-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    font-size: 12px;
    transition: border-color 0.12s, background 0.12s;
  }
  .repo-row:hover {
    border-color: var(--accent);
  }
  .repo-row.selected {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
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
  .dl-badge {
    font-size: 10px;
    color: var(--success);
    background: color-mix(in srgb, var(--success) 12%, transparent);
    border-radius: 4px;
    padding: 1px 6px;
    flex-shrink: 0;
  }
  .del-btn {
    background: none;
    border: 1px solid color-mix(in srgb, var(--error) 45%, transparent);
    color: var(--error);
    font-size: 10px;
    padding: 2px 8px;
    border-radius: 5px;
    cursor: pointer;
    flex-shrink: 0;
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
    padding: 4px 8px;
    border-radius: 6px;
    background: var(--surface);
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
