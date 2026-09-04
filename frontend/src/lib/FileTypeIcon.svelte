<script lang="ts">
  /**
   * 文件类型图标（内联 SVG，跟随 currentColor，无 emoji）。
   * 按扩展名映射：model / image / video / audio / file
   */
  let { path, size = 13 }: { path: string; size?: number } = $props()

  const MODEL_EXTS = ['.blend', '.glb', '.gltf', '.fbx', '.obj', '.abc']
  const IMAGE_EXTS = ['.png', '.jpg', '.jpeg', '.webp', '.gif', '.bmp', '.psd', '.kra', '.sai', '.clip', '.tga', '.exr']
  const VIDEO_EXTS = ['.mp4', '.mov', '.webm', '.avi', '.mkv', '.prproj', '.aep', '.drp']
  const AUDIO_EXTS = ['.wav', '.mp3', '.flac', '.aiff', '.ogg', '.m4a']

  const kind = $derived.by(() => {
    const lower = (path || '').toLowerCase()
    if (MODEL_EXTS.some((e) => lower.endsWith(e))) return 'model'
    if (IMAGE_EXTS.some((e) => lower.endsWith(e))) return 'image'
    if (VIDEO_EXTS.some((e) => lower.endsWith(e))) return 'video'
    if (AUDIO_EXTS.some((e) => lower.endsWith(e))) return 'audio'
    return 'file'
  })
</script>

<svg
  viewBox="0 0 16 16"
  width={size}
  height={size}
  fill="none"
  stroke="currentColor"
  stroke-width="1.3"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if kind === 'model'}
    <path d="M8 1.9 13.6 5v6.1L8 14.1 2.4 11.1V5L8 1.9z" />
    <path d="M2.4 5 8 8l5.6-3M8 8v6.1" />
  {:else if kind === 'image'}
    <rect x="2.2" y="3" width="11.6" height="10" rx="1.5" />
    <circle cx="5.8" cy="6.4" r="1.1" />
    <path d="M2.6 11.6 6.1 8.5l2.3 2.1 2.4-2.2 2.6 2.4" />
  {:else if kind === 'video'}
    <rect x="2" y="3" width="12" height="10" rx="1.6" />
    <path d="M6.9 6.1 10.3 8l-3.4 1.9V6.1z" fill="currentColor" stroke="none" />
  {:else if kind === 'audio'}
    <path d="M6.2 12V4.3l5.8-1.3v7.6" />
    <circle cx="4.7" cy="12" r="1.5" />
    <circle cx="10.5" cy="10.6" r="1.5" />
  {:else}
    <path d="M9.3 1.9H5c-.5 0-.9.4-.9.9v10.4c0 .5.4.9.9.9h6c.5 0 .9-.4.9-.9V4.4L9.3 1.9z" />
    <path d="M9.3 1.9v2.5h2.5" />
  {/if}
</svg>
