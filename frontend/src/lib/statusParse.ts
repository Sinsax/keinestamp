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

// ---- 隐藏规则匹配（与后端 hidden.rs is_ignored 对齐，lore .loreignore 语义）----

/** path 是否被规则忽略：规则按序求值、最后命中者生效（`!` 前缀取反） */
export function ruleHidden(path: string, rules: Iterable<string>): boolean {
  let ignored = false
  for (const raw of rules) {
    const rule = raw.trim()
    if (!rule || rule.startsWith('#')) continue
    const negated = rule.startsWith('!')
    const r = negated ? rule.slice(1).trim() : rule
    if (!r) continue
    if (ruleHiddenOne(r, path)) ignored = !negated
  }
  return ignored
}

/** 返回 path 命中的最后一条规则（生效规则；无则空串）——用于"取消忽略"时删除正确规则 */
export function findRuleHidden(path: string, rules: Iterable<string>): string {
  let found = ''
  for (const raw of rules) {
    const rule = raw.trim()
    if (!rule || rule.startsWith('#')) continue
    const r = rule.startsWith('!') ? rule.slice(1).trim() : rule
    if (r && ruleHiddenOne(r, path)) found = rule
  }
  return found
}

/** 单条规则匹配（lore 语义）：首 `/` 根锚定、尾 `/` 目录（任意层级段名）、无 `/` 裸规则 =
 *  任意层级段名（精确相等）、含 `/` = 根级段前缀匹配（`**` 递归、`*`/`?` 段内通配）、
 *  大小写不敏感 */
function ruleHiddenOne(rule: string, path: string): boolean {
  const pathNorm = path.replace(/\\/g, '/').toLowerCase()
  const ruleLower = rule.toLowerCase()
  const rootAnchored = ruleLower.startsWith('/')
  const r = rootAnchored ? ruleLower.slice(1) : ruleLower
  const dirRule = r.endsWith('/')
  const r2 = dirRule ? r.slice(0, -1) : r
  if (!r2) return false
  const ruleSegs = r2.split('/')
  const segs = pathNorm.split('/')
  if (dirRule) {
    if (rootAnchored) return segs[0] === r2
    return segs.some((s) => s === r2)
  }
  if (rootAnchored) return segmentsPrefixMatch(ruleSegs, segs)
  if (ruleSegs.length === 1) {
    if (ruleSegs[0] === '**') return true
    return segs.some((s) => segmentMatch(ruleSegs[0], s))
  }
  return segmentsPrefixMatch(ruleSegs, segs)
}

/** 段序列前缀匹配（规则段是路径段的前缀；`**` 匹配零或多个段；规则耗尽即命中——目录下全部路径随忽略） */
function segmentsPrefixMatch(pattern: string[], text: string[]): boolean {
  if (pattern.length === 0) return true
  if (pattern[0] === '**') {
    for (let skip = 0; skip <= text.length; skip++) {
      if (segmentsPrefixMatch(pattern.slice(1), text.slice(skip))) return true
    }
    return false
  }
  if (text.length === 0) return false
  return segmentMatch(pattern[0], text[0]) && segmentsPrefixMatch(pattern.slice(1), text.slice(1))
}

/** 单段匹配（`*`/`?` 通配；调用前已小写化） */
function segmentMatch(pattern: string, text: string): boolean {
  return wildcardMatch(pattern, text)
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
