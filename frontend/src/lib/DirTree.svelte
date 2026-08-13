<script lang="ts">
  import DirTreeNode from './DirTreeNode.svelte'
  import { buildFileTree, type FileTreeNode } from './fileTree'

  let {
    paths = [],
    dirs = [],
    activePath = '',
    onContextMenu = (_path: string, _isDir: boolean, _x: number, _y: number) => {},
  }: {
    paths?: string[]
    dirs?: string[]
    activePath?: string
    onContextMenu?: (path: string, isDir: boolean, x: number, y: number) => void
  } = $props()

  const dirSet = $derived(
    new Set(dirs.map((d) => (d.endsWith('/') ? d.slice(0, -1) : d)))
  )
  const tree = $derived(buildFileTree(paths, dirSet))

  function collectDirPaths(nodes: FileTreeNode[]): string[] {
    const dirs: string[] = []
    for (const n of nodes) {
      // isDir（含空文件夹，emptyDirs 传入）或 children.length > 0（文件下的中间目录）都算目录
      if (n.isDir || n.children.length > 0) {
        dirs.push(n.path)
        dirs.push(...collectDirPaths(n.children))
      }
    }
    return dirs
  }

  let collapsed = $state<Set<string>>(new Set())
  let inited = $state(false)
  // 用户手动展开/折叠过的目录：后续树更新（emptyDirs 晚到、文件新增等）不自动改它们
  let userTouched = $state<Set<string>>(new Set())
  // 项目切换（activePath = 项目根路径）→ 重置初始化标志，新树就绪后重新默认全折叠
  let prevProject = $state<string | null>(null)

  $effect(() => {
    if (activePath !== prevProject) {
      prevProject = activePath
      inited = false
      userTouched = new Set()
      collapsed = new Set()
    }
  })

  // 树内容变化时：
  // - 初始化（inited=false）：全折叠
  // - 树更新（如 emptyDirs 晚到）：新增目录默认补折叠；用户手动操作过的目录保持原状
  $effect(() => {
    const all = collectDirPaths(tree)
    if (all.length === 0) return
    const next = new Set(collapsed)
    let changed = false
    if (!inited) {
      for (const d of all) next.add(d)
      inited = true
      changed = true
    } else {
      for (const d of all) {
        if (!next.has(d) && !userTouched.has(d)) {
          next.add(d)
          changed = true
        }
      }
    }
    if (changed) collapsed = next
  })

  function toggle(p: string) {
    const next = new Set(collapsed)
    if (next.has(p)) next.delete(p)
    else next.add(p)
    collapsed = next
    userTouched = new Set(userTouched).add(p)
  }
</script>

{#if tree.length > 0}
  {#each tree as node (node.path)}
    <DirTreeNode {node} {collapsed} {toggle} depth={0} {activePath} {onContextMenu} />
  {/each}
{/if}
