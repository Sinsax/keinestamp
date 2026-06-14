<script lang="ts">
  import DirTreeNode from './DirTreeNode.svelte'
  import { buildFileTree, type FileTreeNode } from './fileTree'

  let { paths = [] }: { paths?: string[] } = $props()

  const tree = $derived(buildFileTree(paths))

  function collectDirPaths(nodes: FileTreeNode[]): string[] {
    const dirs: string[] = []
    for (const n of nodes) {
      if (n.children.length > 0) {
        dirs.push(n.path)
        dirs.push(...collectDirPaths(n.children))
      }
    }
    return dirs
  }

  let collapsed = $state<Set<string>>(new Set())
  let inited = $state(false)

  $effect(() => {
    if (!inited && tree.length > 0) {
      collapsed = new Set(collectDirPaths(tree))
      inited = true
    }
  })

  function toggle(p: string) {
    const next = new Set(collapsed)
    if (next.has(p)) next.delete(p)
    else next.add(p)
    collapsed = next
  }
</script>

{#if tree.length > 0}
  {#each tree as node (node.path)}
    <DirTreeNode {node} {collapsed} {toggle} depth={0} />
  {/each}
{/if}
