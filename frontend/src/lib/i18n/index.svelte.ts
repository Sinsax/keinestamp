/**
 * 统一多语言运行时（前端）。
 *
 * 文案真源在仓库根 `i18n/{zh-CN,en-US}.json`，与 Rust（`src/i18n.rs`）共用同一份文件。
 * key 类型由 zh-CN.json 的结构推导，写错即编译失败。
 * 具体语言由 Rust 侧解析（env → AppConfig → 系统语言），前端只负责渲染。
 */
import zhCN from '../../../../i18n/zh-CN.json'
import enUS from '../../../../i18n/en-US.json'

export type Locale = 'zh-CN' | 'en-US'
/** 用户偏好：跟随系统 / 固定语言 */
export type LocalePref = 'system' | Locale

type Leaves<T> = {
  [K in keyof T & string]: T[K] extends string ? K : `${K}.${Leaves<T[K]>}`
}[keyof T & string]

/** 所有合法文案 key，如 'gui.settings.language' */
export type Key = Leaves<typeof zhCN>

/** 复数键基名（对应 `key.one` / `key.other`） */
type StripPlural<K extends string> = K extends `${infer Base}.${'one' | 'other'}` ? Base : never
export type PluralKey = StripPlural<Key>

type Dict = Record<string, string>

function flatten(source: unknown, prefix = '', out: Dict = {}): Dict {
  if (source && typeof source === 'object') {
    for (const [key, value] of Object.entries(source as Record<string, unknown>)) {
      flatten(value, prefix ? `${prefix}.${key}` : key, out)
    }
  } else if (typeof source === 'string' && prefix) {
    out[prefix] = source
  }
  return out
}

const dicts: Record<Locale, Dict> = {
  'zh-CN': flatten(zhCN),
  'en-US': flatten(enUS),
}

let current = $state<Locale>('zh-CN')
// 语言版本号：每次切换 +1，供已挂载的界面检测"后端文案需要重新拉取"
let epoch = $state(0)
let systemTag = $state<Locale>('zh-CN')

export function getLocale(): Locale {
  return current
}

/** 系统语言（设置页展示"跟随系统（当前：English）"用） */
export function getSystemLocale(): Locale {
  return systemTag
}

export function setLocale(locale: Locale): void {
  if (current !== locale) epoch += 1
  current = locale
  if (typeof document !== 'undefined') document.documentElement.lang = locale
}

/** 语言版本号（切换语言时递增） */
export function localeEpoch(): number {
  return epoch
}

export function syncSystemLocale(locale: Locale): void {
  systemTag = locale
}

export function nativeName(locale: Locale): string {
  return locale === 'zh-CN' ? '简体中文' : 'English'
}

/**
 * 取文案。读取 `current` 这个 $state，因此切换语言时所有调用点自动重渲染。
 * 非中文界面缺键时回落中文，最后才回落 key 本身。
 */
export function t(key: Key, params?: Record<string, string | number>): string {
  const dict = dicts[current]
  let text = dict[key] ?? dicts['zh-CN'][key] ?? (key as string)
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      text = text.replace(`{${name}}`, String(value))
    }
  }
  return text
}

/** 复数：中文两句同文，英文按 n === 1 选 `one` */
export function tn(key: PluralKey, n: number, params?: Record<string, string | number>): string {
  const suffix = n === 1 ? 'one' : 'other'
  return t(`${key}.${suffix}` as Key, { n, ...params })
}
