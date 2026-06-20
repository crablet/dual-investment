// 按本地时区生成日期键，避免 toISOString() 的 UTC 转换改变日历日期。

import { dateEo } from 'naive-ui'
import { formatLocale, i18n } from '@/i18n'

function esperantoMonth(date: Date) {
  return dateEo.locale.localize.month(date.getMonth() as Parameters<typeof dateEo.locale.localize.month>[0], { width: 'wide' })
}

/** 日历标题使用当前语言；世界语直接复用组件库日期词典，不依赖 WebView 的 ICU 覆盖。 */
export function calendarMonthLabel(date: Date) {
  if (i18n.global.locale.value === 'eo')
    return `${esperantoMonth(date)} ${date.getFullYear()}`
  return new Intl.DateTimeFormat(formatLocale(), { year: 'numeric', month: 'long', calendar: 'gregory' }).format(date)
}

export function calendarDateLabel(date: Date) {
  if (i18n.global.locale.value === 'eo') {
    const weekday = dateEo.locale.localize.day(date.getDay() as Parameters<typeof dateEo.locale.localize.day>[0], { width: 'wide' })
    return `${weekday}, ${date.getDate()} ${esperantoMonth(date)}`
  }
  return new Intl.DateTimeFormat(formatLocale(), { month: 'long', day: 'numeric', weekday: 'long', calendar: 'gregory' }).format(date)
}

/** 按本地时区格式化 YYYY-MM-DD。 */
export function formatDateKey(date: Date) {
  const year = date.getFullYear()
  const month = `${date.getMonth() + 1}`.padStart(2, '0')
  const day = `${date.getDate()}`.padStart(2, '0')
  return `${year}-${month}-${day}`
}

/** 解析本地日期键。 */
export function parseDateKey(dateKey: string) {
  const [year, month, day] = dateKey.split('-').map(Number)
  return new Date(year, month - 1, day)
}

/** 返回本地当天 00:00。 */
export function startOfDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate())
}

/** 返回本地月份第一天。 */
export function startOfMonth(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), 1)
}

/** 返回所在周的周日。 */
export function startOfWeek(date: Date) {
  return addDays(date, -date.getDay())
}

/** 日期加天数，保留本地时区语义。 */
export function addDays(date: Date, days: number) {
  const next = new Date(date)
  next.setDate(next.getDate() + days)
  return next
}

/** 月份加减，结果固定为目标月份第一天。 */
export function addMonths(date: Date, months: number) {
  return new Date(date.getFullYear(), date.getMonth() + months, 1)
}

/** 判断两个日期是否处于同一自然月。 */
export function isSameMonth(left: Date, right: Date) {
  return left.getFullYear() === right.getFullYear() && left.getMonth() === right.getMonth()
}

/** 把 ISO 时间转成本地日期键；解析失败时保留日期前缀。 */
export function localDateKey(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return value.slice(0, 10)
  }
  return formatDateKey(date)
}

/** 把 ISO 时间转成本地 HH:mm 标签。 */
export function localTimeLabel(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return undefined
  }
  return date.toLocaleTimeString(formatLocale(), {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  })
}

/** 把 ISO 时间转成本地 YYYY-MM-DD HH:mm:ss 标签。 */
export function localDateTimeLabel(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return value
  }
  return `${formatDateKey(date)} ${date.toLocaleTimeString(formatLocale(), {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })}`
}
