<script lang="ts">
  let { toasts }: { toasts: { id: number; msg: string; type: string }[] } = $props()
</script>

{#if toasts.length > 0}
  <div class="toast-container">
    {#each toasts as t (t.id)}
      <div class="toast {t.type}">
        <span class="toast-icon">
          {#if t.type === 'success'}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M13.854 3.646a.5.5 0 010 .708l-7 7a.5.5 0 01-.708 0l-3.5-3.5a.5.5 0 11.708-.708L6.5 10.293l6.646-6.647a.5.5 0 01.708 0z"/></svg>
          {:else if t.type === 'error'}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 1a7 7 0 107 7 7 7 0 00-7-7zm3.354 9.646a.5.5 0 01-.708.708L8 8.707l-2.646 2.647a.5.5 0 01-.708-.708L7.293 8 4.646 5.354a.5.5 0 01.708-.708L8 7.293l2.646-2.647a.5.5 0 01.708.708L8.707 8z"/></svg>
          {:else}
            <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M8 15A7 7 0 118 1a7 7 0 010 14zm0 1A8 8 0 108 0a8 8 0 000 16z"/><path d="M8 4a.5.5 0 01.5.5v5a.5.5 0 01-1 0v-5A.5.5 0 018 4zm0 7.5a.75.75 0 110 1.5.75.75 0 010-1.5z"/></svg>
          {/if}
        </span>
        <span class="toast-msg">{t.msg}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-icon {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
  }
  .toast.success .toast-icon { color: var(--ze); }
  .toast.error .toast-icon { color: var(--error); }
  .toast.info .toast-icon { color: var(--ice); }
  .toast-msg { flex: 1; }
</style>
