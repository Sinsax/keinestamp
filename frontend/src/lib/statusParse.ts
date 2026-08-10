export interface StatusFile {
  path: string
  x: string
  y: string
}

// Tauri status 输出格式（lore 后端）：
//   `未提交的修改：` 节头 + `  {code} {path}` 行，code ∈ A/M/D/??
export function parseStatus(output: string): StatusFile[] {
  const result: StatusFile[] = []
  let inChanges = false
  for (const raw of output.split('\n')) {
    const l = raw.trimEnd()
    if (!inChanges) {
      if (l.includes('未提交的修改')) {
        inChanges = true
      }
      continue
    }
    if (l.trim() === '') continue
    const m = l.match(/^  (\S+)\s+(.+)$/)
    if (!m) continue
    const code = m[1]
    const path = m[2].trim()
    if (!path || path === '') continue
    result.push({ x: code === '??' ? '?' : code, y: ' ', path })
  }
  return result
}

export function statusLabel(x: string, y: string): string {
  if (x === 'M') return 'modified'
  if (x === 'A') return 'added'
  if (x === 'D') return 'deleted'
  if (x === '?') return 'untracked'
  if (y === 'M') return 'modified'
  if (y === 'A') return 'added'
  if (y === 'D') return 'deleted'
  if (y === '?') return 'untracked'
  return ''
}

export function statusColor(x: string, y: string): string {
  if (x === 'M') return 'var(--warning)'
  if (x === 'A') return 'var(--success)'
  if (x === 'D') return 'var(--error)'
  if (x === '?') return 'var(--text-dim)'
  if (y === 'M') return 'var(--warning)'
  if (y === 'A') return 'var(--success)'
  if (y === 'D') return 'var(--error)'
  if (y === '?') return 'var(--text-dim)'
  return 'var(--text-dim)'
}

export function statusIconType(x: string, y: string): string {
  if (x === 'A' || y === 'A') return 'added'
  if (x === 'D' || y === 'D') return 'deleted'
  if (x === 'M' || y === 'M') return 'modified'
  if (x === '?' || y === '?') return 'untracked'
  return 'unknown'
}

export function ext(path: string): string {
  const i = path.lastIndexOf('.')
  return i >= 0 ? path.slice(i + 1).toUpperCase() : 'FILE'
}
