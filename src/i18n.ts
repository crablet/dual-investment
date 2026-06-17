// 全局消息翻译入口。消息保留代码和参数，直到展示时才按当前语言解析。
import { createI18n } from 'vue-i18n'
import en from '@/locales/en.json'

export interface LocalizedMessage {
  code: string
  params: Record<string, string | LocalizedMessage>
}

export const supportedLocales = ['zh-CN', 'en', 'zh-TW', 'pt-BR', 'es', 'vi', 'tr', 'id', 'ja', 'ko', 'th', 'eo', 'fr'] as const
export type AppLocale = typeof supportedLocales[number]

const catalogs = import.meta.glob<typeof en>(['./locales/*.json', '!./locales/en.json'], { import: 'default' })
const pendingCatalogs = new Map<AppLocale, Promise<void>>()
const messages: Partial<Record<AppLocale, typeof en>> = { en }

export const i18n = createI18n({
  legacy: false,
  locale: 'en',
  fallbackLocale: 'en',
  messages,
  missingWarn: import.meta.env.DEV,
  fallbackWarn: import.meta.env.DEV,
})

/** 英语始终可用；其余词典按需注册，并合并同一语言的并发加载。 */
export function loadLocale(locale: AppLocale): Promise<void> {
  if (i18n.global.availableLocales.includes(locale))
    return Promise.resolve()

  const pending = pendingCatalogs.get(locale)
  if (pending)
    return pending

  const load = catalogs[`./locales/${locale}.json`]
  if (!load)
    return Promise.reject(new Error(`Unsupported locale: ${locale}`))

  const request = load()
    .then(catalog => i18n.global.setLocaleMessage(locale, catalog))
    .finally(() => pendingCatalogs.delete(locale))
  pendingCatalogs.set(locale, request)
  return request
}

export function t(code: string, params: Record<string, unknown> = {}): string {
  return i18n.global.t(code, params)
}

/** 字符串仅用于本地技术详情；后端业务消息始终携带代码和参数。 */
export function messageText(message: LocalizedMessage | string | null | undefined): string {
  if (!message)
    return ''
  if (typeof message === 'string')
    return message
  const params = Object.fromEntries(Object.entries(message.params).map(([key, value]) => [key, messageText(value)]))
  if (!i18n.global.te(message.code) && !i18n.global.te(message.code, 'en'))
    return t('ui.unknownMessage', { code: message.code })
  return t(message.code, params)
}

export function message(code: string, params: LocalizedMessage['params'] = {}): LocalizedMessage {
  return { code, params }
}

export function resolveLocale(languages: readonly string[]): AppLocale {
  for (const tag of languages) {
    const locale = tag.toLowerCase().replaceAll('_', '-')
    if (locale.startsWith('zh'))
      return /(?:^|-)hant(?:-|$)|(?:^|-)(?:tw|hk|mo)(?:-|$)/.test(locale) ? 'zh-TW' : 'zh-CN'
    if (locale === 'pt' || locale.startsWith('pt-'))
      return 'pt-BR'
    const base = locale.split('-')[0]
    if (supportedLocales.includes(base as AppLocale))
      return base as AppLocale
  }
  return 'en'
}

/** 语言只决定展示惯例；日期仍使用系统本地时区和公历。 */
export function formatLocale(): string {
  const locale = i18n.global.locale.value
  // 部分 WebView 缺少世界语 Intl 数据；数字与 24 小时时间采用相同惯例的法语格式。
  // 日历中的世界语月份和星期由日期词典提供，避免回退到设备默认语言。
  if (locale === 'eo')
    return 'fr'
  return locale === 'th' ? 'th-TH-u-ca-gregory-nu-latn' : locale
}
