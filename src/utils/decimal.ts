// Decimal 字符串展示层工具。
//
// 这些工具只服务前端 UI 派生，避免为了展示聚合把金额或数量先转成 JS number。
import type { DecimalString } from '@/types/domain'

interface ParsedDecimal {
  units: bigint
  scale: number
}

/**
 * 汇总 Decimal 字符串。
 *
 * 用 BigInt 对齐小数位，避免把金额、价格或数量转成 number 后再求和。
 */
export function sumDecimalStrings(values: DecimalString[]): DecimalString {
  const parsed = values.map(parseDecimalString)
  const scale = Math.max(0, ...parsed.map(value => value.scale))
  const total = parsed.reduce((sum, value) => {
    const multiplier = 10n ** BigInt(scale - value.scale)
    return sum + value.units * multiplier
  }, 0n)
  return formatScaledDecimal(total, scale)
}

/** 判断 Decimal 字符串是否为 0。 */
export function isZeroDecimalString(value: DecimalString): boolean {
  return /^-?0(?:\.0+)?$/.test(value.trim())
}

/** 判断 Decimal 字符串是否为负数且非 0。 */
export function isNegativeDecimalString(value: DecimalString): boolean {
  return value.trim().startsWith('-') && !isZeroDecimalString(value)
}

/** 把 Decimal 字符串拆成整数单位和小数位数。非法输入按 0 处理，避免 UI 派生中断。 */
function parseDecimalString(raw: DecimalString): ParsedDecimal {
  const matched = raw.trim().match(/^([+-])?(\d+)(?:\.(\d+))?$/)
  if (!matched) {
    return { units: 0n, scale: 0 }
  }
  const [, sign, integer, fraction = ''] = matched
  const digits = `${integer}${fraction}`.replace(/^0+(?=\d)/, '') || '0'
  const direction = sign === '-' ? -1n : 1n
  return {
    units: BigInt(digits) * direction,
    scale: fraction.length,
  }
}

/** 把对齐后的整数单位格式化回 Decimal 字符串。 */
function formatScaledDecimal(units: bigint, scale: number): DecimalString {
  if (scale === 0) {
    return units.toString()
  }
  const sign = units < 0n ? '-' : ''
  const digits = (units < 0n ? -units : units).toString().padStart(scale + 1, '0')
  const integer = digits.slice(0, -scale)
  const fraction = digits.slice(-scale).replace(/0+$/, '')
  return fraction ? `${sign}${integer}.${fraction}` : `${sign}${integer}`
}
