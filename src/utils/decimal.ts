// Decimal 字符串的展示、比较与排序工具；金额计算由 Rust 负责。
import type { DecimalString } from '@/types/domain'

/** 判断 Decimal 字符串是否为 0。 */
export function isZeroDecimalString(value: DecimalString): boolean {
  return /^-?0(?:\.0+)?$/.test(value.trim())
}

/** 判断 Decimal 字符串是否为负数且非 0。 */
export function isNegativeDecimalString(value: DecimalString): boolean {
  return value.trim().startsWith('-') && !isZeroDecimalString(value)
}

/** 仅供图形几何和 Intl 格式化使用，返回值不能参与金额、数量或收益计算。 */
export function decimalStringToDisplayNumber(value: DecimalString): number {
  const numeric = Number(value)
  return Number.isFinite(numeric) ? numeric : 0
}

/** 判断 Decimal 字符串是否有非零值。 */
export function decimalStringHasValue(value: DecimalString): boolean {
  return !isZeroDecimalString(value)
}

/** 按十进制文本比较大小；筛选和排序不必先丢失精度转成 number。 */
export function compareDecimalStrings(left: DecimalString, right: DecimalString): number | undefined {
  const a = decimalParts(left)
  const b = decimalParts(right)
  if (!a || !b) {
    return undefined
  }
  if (a.negative !== b.negative) {
    return a.negative ? -1 : 1
  }
  const fractionLength = Math.max(a.fraction.length, b.fraction.length)
  const order = a.integer.length - b.integer.length
    || compareText(a.integer, b.integer)
    || compareText(a.fraction.padEnd(fractionLength, '0'), b.fraction.padEnd(fractionLength, '0'))
  return a.negative ? -Math.sign(order) : Math.sign(order)
}

export function isPositiveDecimalString(value: DecimalString): boolean {
  return compareDecimalStrings(value, '0') === 1
}

function decimalParts(raw: DecimalString) {
  const matched = raw.trim().match(/^([+-])?(\d+)(?:\.(\d+))?$/)
  if (!matched) {
    return undefined
  }
  const integer = matched[2].replace(/^0+(?=\d)/, '')
  const fraction = (matched[3] ?? '').replace(/0+$/, '')
  return {
    integer,
    fraction,
    negative: matched[1] === '-' && (integer !== '0' || fraction !== ''),
  }
}

function compareText(left: string, right: string) {
  return left === right ? 0 : left < right ? -1 : 1
}
