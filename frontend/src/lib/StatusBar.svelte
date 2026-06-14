<script lang="ts">
  let { text = '' }: { text?: string } = $props()
</script>

<div class="status-bar">{text || 'Ready'}</div>
