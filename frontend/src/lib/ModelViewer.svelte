<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import * as THREE from 'three'
  import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
  import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
  import { convertFileSrc } from '@tauri-apps/api/core'

  let {
    glbPath,
  }: {
    glbPath: string
  } = $props()

  let container: HTMLDivElement
  let loading = $state(true)
  let loadError = $state('')

  let renderer: THREE.WebGLRenderer
  let scene: THREE.Scene
  let camera: THREE.PerspectiveCamera
  let controls: OrbitControls
  let animId: number

  function sanitizeUrl(p: string): string {
    if (!p) return ''
    if (p.startsWith('http://') || p.startsWith('https://') || p.startsWith('asset://')) return p
    try { return convertFileSrc(p) } catch { return p }
  }

  onMount(() => {
    if (!container) return

    const w = container.clientWidth
    const h = 280

    scene = new THREE.Scene()
    scene.background = new THREE.Color(0x1a1a2e)

    camera = new THREE.PerspectiveCamera(45, w / h, 0.1, 1000)
    camera.position.set(3, 2, 5)

    renderer = new THREE.WebGLRenderer({ antialias: true })
    renderer.setSize(w, h)
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    renderer.toneMapping = THREE.ACESFilmicToneMapping
    renderer.toneMappingExposure = 1.0
    container.appendChild(renderer.domElement)

    controls = new OrbitControls(camera, renderer.domElement)
    controls.enableDamping = true
    controls.dampingFactor = 0.1
    controls.autoRotate = true
    controls.autoRotateSpeed = 2.0
    controls.target.set(0, 0, 0)
    controls.update()

    const ambient = new THREE.AmbientLight(0xffffff, 0.5)
    scene.add(ambient)

    const key = new THREE.DirectionalLight(0xffffff, 1.5)
    key.position.set(5, 8, 5)
    scene.add(key)

    const fill = new THREE.DirectionalLight(0x8888ff, 0.4)
    fill.position.set(-3, 2, -3)
    scene.add(fill)

    const rim = new THREE.DirectionalLight(0xffffff, 0.6)
    rim.position.set(0, -3, 4)
    scene.add(rim)

    const grid = new THREE.GridHelper(10, 10, 0x444466, 0x333355)
    scene.add(grid)

    const url = sanitizeUrl(glbPath)
    if (!url) {
      loadError = 'Invalid file path'
      loading = false
      return
    }

    const loader = new GLTFLoader()
    loader.load(
      url,
      (gltf) => {
        const model = gltf.scene
        const box = new THREE.Box3().setFromObject(model)
        const size = box.getSize(new THREE.Vector3())
        const center = box.getCenter(new THREE.Vector3())
        const maxDim = Math.max(size.x, size.y, size.z)

        if (maxDim > 0) {
          const scale = 2.5 / maxDim
          model.scale.set(scale, scale, scale)
        }
        model.position.sub(center.clone().multiplyScalar(model.scale.x))

        scene.add(model)
        loading = false
      },
      undefined,
      (err) => {
        loadError = `Failed to load model: ${err instanceof Error ? err.message : 'Unknown error'}`
        loading = false
      }
    )

    function animate() {
      animId = requestAnimationFrame(animate)
      controls.update()
      renderer.render(scene, camera)
    }
    animate()
  })

  onDestroy(() => {
    if (animId) cancelAnimationFrame(animId)
    if (renderer) {
      renderer.dispose()
      renderer.domElement.remove()
    }
    if (controls) controls.dispose()
  })
</script>

<div class="model-viewer" bind:this={container}>
  {#if loading}
    <div class="model-overlay">Loading 3D model...</div>
  {/if}
  {#if loadError}
    <div class="model-overlay error">{loadError}</div>
  {/if}
</div>

<style>
  .model-viewer {
    width: 100%;
    height: 280px;
    border-radius: 6px;
    overflow: hidden;
    position: relative;
    background: #1a1a2e;
  }
  .model-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #ccc;
    font-size: 12px;
    background: rgba(0,0,0,0.4);
    z-index: 1;
    pointer-events: none;
  }
  .model-overlay.error {
    color: #ff6b6b;
  }
</style>
