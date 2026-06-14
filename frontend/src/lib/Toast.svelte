<script lang="ts">
  let { toasts }: { toasts: { id: number; msg: string; type: string }[] } = $props()
</script>

{#if toasts.length > 0}
  <div class="toast-container">
    {#each toasts as t (t.id)}
      <div class="toast {t.type}">{t.msg}</div>
    {/each}
  </div>
{/if}
