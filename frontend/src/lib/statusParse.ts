export interface StatusFile {
  path: string
  x: string
  y: string
}

function unquoteC(p: string): string {
  if (p.length < 2 || p[0] !== '"' || p[p.length - 1] !== '"') return p
  const s = p.slice(1, -1)
  return s.replace(/\\(?:(["\\abtnr])|(\d{3}))/g, (m, c, o) => {
    if (o) return String.fromCharCode(parseInt(o, 8))
    switch (c) {
      case 'a': return '\x07'
      case 'b': return '\b'
      case 't': return '\t'
      case 'n': return '\n'
      case 'r': return '\r'
      default: return c
    }
  })
}

export function parseStatus(output: string): StatusFile[] {
  return output
    .split('\n')
    .filter((l) => l.trim().length > 0)
    .map((l) => {
      const x = l[0] ?? ' '
      const y = l[1] ?? ' '
      let path = l.length > 3 ? l.substring(3) : ''
      if (x === 'R' || y === 'R') {
        const arrow = path.lastIndexOf(' -> ')
        if (arrow >= 0) path = path.slice(arrow + 4)
      }
      return { x, y, path: unquoteC(path.trim()) }
    })
}

export function statusLabel(x: string, y: string): string {
  if (x !== ' ') return 'staged'
  if (y === 'M') return 'modified'
  if (y === 'A') return 'added'
  if (y === 'D') return 'deleted'
  if (y === '?') return 'untracked'
  return ''
}

export function statusColor(x: string, y: string): string {
  if (x !== ' ') {
    if (x === 'M') return 'var(--accent)'
    if (x === 'A') return 'var(--success)'
    if (x === 'D') return 'var(--error)'
  }
  if (y === 'M') return 'var(--warning)'
  if (y === 'A') return 'var(--success)'
  if (y === 'D') return 'var(--error)'
  if (y === '?') return 'var(--text-dim)'
  return 'var(--text-dim)'
}

export function statusIconType(x: string, y: string): string {
  if (x === 'A' || y === 'A') return 'added'
  if (x === 'D' || y === 'D') return 'deleted'
  if (x !== ' ' || y === 'M') return 'modified'
  if (y === '?') return 'untracked'
  return 'unknown'
}

export function ext(path: string): string {
  const i = path.lastIndexOf('.')
  return i >= 0 ? path.slice(i + 1).toUpperCase() : 'FILE'
}
