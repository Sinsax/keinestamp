export interface FileTreeNode {
  name: string
  path: string
  isDir: boolean
  children: FileTreeNode[]
}

function sortTree(nodes: FileTreeNode[]) {
  nodes.sort((a, b) => {
    const dirA = a.isDir || a.children.length > 0
    const dirB = b.isDir || b.children.length > 0
    if (dirA && !dirB) return -1
    if (!dirA && dirB) return 1
    return a.name.localeCompare(b.name)
  })
  for (const n of nodes) {
    if (n.children.length > 0) sortTree(n.children)
  }
}

export function buildFileTree(paths: string[], dirs?: Iterable<string>): FileTreeNode[] {
  const dirSet = new Set(dirs ?? [])
  const root: FileTreeNode[] = []

  for (const fullPath of paths) {
    const normalized = fullPath.replace(/\\/g, '/')
    const parts = normalized.split('/').filter(Boolean)
    let current = root

    for (let i = 0; i < parts.length; i++) {
      const part = parts[i]
      let node = current.find((n) => n.name === part)

      if (!node) {
        const nodePath = parts.slice(0, i + 1).join('/')
        node = {
          name: part,
          path: nodePath,
          isDir: dirSet.has(nodePath),
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
