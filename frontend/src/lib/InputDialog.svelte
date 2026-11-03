<script lang="ts">
  import { t } from './i18n/index.svelte'

  let {
    title,
    label = '',
    initial = '',
    okLabel = '',
    cancelLabel = '',
    mustEqual = '',
    danger = false,
    uppercase = false,
    onConfirm,
    onCancel,
  }: {
    title: string
    label?: string
    initial?: string
    okLabel?: string
    cancelLabel?: string
    /** 输入必须精确等于该值才允许确认（如 "DELETE"） */
    mustEqual?: string
    /** 确认按钮红色（危险操作） */
    danger?: boolean
    /** 输入自动转大写（如 DELETE 确认框） */
    uppercase?: boolean
    onConfirm: (value: string) => void
    onCancel: () => void
  } = $props()

  const okText = $derived(okLabel || t('common.confirm'))
  const cancelText = $derived(cancelLabel || t('common.cancel'))
  const invalidHint = $derived(
    mustEqual ? t('gui.input.mustEqual', { value: mustEqual }) : t('gui.input.invalidDirName')
  )

  let value = $state('')
  let inputEl = $state<HTMLInputElement | null>(null)

  const invalid = $derived(
    mustEqual ? value !== mustEqual : value.includes('/') || value.includes('\\')
  )

  function handleInput(e: Event) {
    if (uppercase) {
      const el = e.currentTarget as HTMLInputElement
      value = el.value.toUpperCase()
      el.value = value
    }
  }

  $effect(() => {
    value = initial
    inputEl?.focus()
    inputEl?.select()
  })

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !invalid) {
      e.preventDefault()
      onConfirm(value)
    } else if (e.key === 'Escape') {
      e.preventDefault()
      onCancel()
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="dialog-overlay" onclick={onCancel} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="dialog input-dialog" onclick={(e: MouseEvent) => e.stopPropagation()} role="dialog" tabindex="-1">
    <h3>{title}</h3>
    <div class="field">
      <label for="input-dialog-field">{label}</label>
      <input id="input-dialog-field" bind:this={inputEl} type="text" bind:value oninput={handleInput} placeholder={t('gui.input.placeholder')} />
      {#if invalid}
        <div class="input-err">{invalidHint}</div>
      {/if}
    </div>
    <div class="actions">
      <button onclick={onCancel}>{cancelText}</button>
      <button onclick={() => onConfirm(value)} class="accent" class:danger disabled={invalid}>{okText}</button>
    </div>
  </div>
</div>

<style>
  :global(.input-dialog) {
    width: 420px;
  }
  .input-err {
    font-size: 11px;
    color: var(--error);
    margin-top: 4px;
  }
</style>

