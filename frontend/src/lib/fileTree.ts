export interface FileTreeNode {
  name: string
  path: string
  children: FileTreeNode[]
}

function sortTree(nodes: FileTreeNode[]) {
  nodes.sort((a, b) => {
    if (a.children.length > 0 && b.children.length === 0) return -1
    if (a.children.length === 0 && b.children.length > 0) return 1
    return a.name.localeCompare(b.name)
  })
  for (const n of nodes) {
    if (n.children.length > 0) sortTree(n.children)
  }
}

export function buildFileTree(paths: string[]): FileTreeNode[] {
  const root: FileTreeNode[] = []

  for (const fullPath of paths) {
    const normalized = fullPath.replace(/\\/g, '/')
    const parts = normalized.split('/').filter(Boolean)
    let current = root

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i]
      let node = current.find((n) => n.name === part)

      if (!node) {
        node = {
          name: part,
          path: parts.slice(0, i + 1).join('/'),
          children: [],
        }
        current.push(node)
      }

      current = node.children
    }
  }

  sortTree(root)
  return root
}
