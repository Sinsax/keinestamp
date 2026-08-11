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
  if (x === 'M' || y === 'M') return '已修改'
  if (x === 'A' || y === 'A') return '新增'
  if (x === 'D' || y === 'D') return '已删除'
  if (x === '?' || y === '?') return '未跟踪'
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

// ---- 隐藏规则匹配（与后端 hidden.rs is_hidden 同语义）----

/** path 是否命中任一规则：含 `*`/`?` 的按通配匹配（无 `/` 规则匹配任意层级文件名），否则前缀匹配 */
export function ruleHidden(path: string, rules: Iterable<string>): boolean {
  for (const rule of rules) {
    if (ruleHiddenOne(rule, path)) return true
  }
  return false
}

/** 返回 path 命中的第一条规则（无则空串）——用于"取消忽略"时删除正确规则 */
export function findRuleHidden(path: string, rules: Iterable<string>): string {
  for (const rule of rules) {
    if (ruleHiddenOne(rule, path)) return rule
  }
  return ''
}

function ruleHiddenOne(rule: string, path: string): boolean {
  if (rule.includes('*') || rule.includes('?')) {
    const ruleSegs = rule.split('/')
    if (ruleSegs.length === 1) {
      const file = path.split('/').pop() ?? path
      return wildcardMatch(rule, file)
    }
    const pathSegs = path.split('/')
    return (
      pathSegs.length === ruleSegs.length &&
      ruleSegs.every((r, i) => wildcardMatch(r, pathSegs[i]))
    )
  }
  if (rule.endsWith('/')) {
    // 目录规则：`cache/` 匹配目录本身（`cache`）与其下所有路径
    return path === rule.slice(0, -1) || path.startsWith(rule)
  }
  return path === rule || path.startsWith(rule)
}

/** 经典贪心通配符匹配：`*` 匹配任意字符序列，`?` 匹配单字符 */
function wildcardMatch(pattern: string, text: string): boolean {
  let pi = 0
  let ti = 0
  let star = -1
  let mark = 0
  while (ti < text.length) {
    if (pi < pattern.length && (pattern[pi] === '?' || pattern[pi] === text[ti])) {
      pi++
      ti++
    } else if (pi < pattern.length && pattern[pi] === '*') {
      star = pi
      mark = ti
      pi++
    } else if (star !== -1) {
      pi = star + 1
      mark++
      ti = mark
    } else {
      return false
    }
  }
  while (pi < pattern.length && pattern[pi] === '*') pi++
  return pi === pattern.length
}
