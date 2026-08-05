export interface StatusFile {
  path: string
  x: string
  y: string
}

export function parseStatus(output: string): StatusFile[] {
  return output
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l.length > 0)
    .map((l) => ({
      x: l[0] || ' ',
      y: l[1] || ' ',
      path: l.substring(3).trim(),
    }))
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
