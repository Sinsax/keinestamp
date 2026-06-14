<script lang="ts">
  let { onConfirm, onCancel }: { onConfirm: (path: string) => void; onCancel: () => void } = $props()

  let path = $state('')

  function handleConfirm() {
    if (path.trim()) onConfirm(path.trim())
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') handleConfirm()
    if (e.key === 'Escape') onCancel()
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="dialog-overlay" onclick={onCancel} role="dialog">
  <div class="dialog" onclick={(e: MouseEvent) => e.stopPropagation()} role="document">
    <h3>Init New Project</h3>
    <div class="field">
      <label>Project path</label>
      <input
        type="text"
        bind:value={path}
        placeholder="e.g. C:\Projects\my_blend"
      />
    </div>
    <div class="actions">
      <button onclick={onCancel}>Cancel</button>
      <button onclick={handleConfirm} class="accent" disabled={!path.trim()}>
        Init
      </button>
    </div>
  </div>
</div>
