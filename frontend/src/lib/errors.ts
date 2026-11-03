/**
 * 后端错误码解析。
 *
 * 关键错误在 Rust 侧用 `[code:xxx]人类可读消息` 的形式返回（见 src/i18n.rs）。
 * 前端只按 code 做分支，按消息做展示 —— 控制流不能依赖自然语言，
 * 否则切换界面语言后引导逻辑（如"离线仓库 → 引导迁移"）会静默失效。
 */
export const ERROR_CODE_PREFIX = '[code:'

export function errorCode(message: string): string | null {
  if (!message.startsWith(ERROR_CODE_PREFIX)) return null
  const end = message.indexOf(']')
  if (end < 0) return null
  return message.slice(ERROR_CODE_PREFIX.length, end)
}

/** 去掉错误码前缀，只留可展示消息 */
export function stripErrorCode(message: string): string {
  const code = errorCode(message)
  if (code === null) return message
  return message.slice(message.indexOf(']') + 1).trimStart()
}
