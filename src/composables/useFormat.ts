// 展示格式化工具。
//
// 这些函数只用于 UI 格式化；金额、数量和收益计算都由 Rust Decimal 完成。

import type { StrategyDirection, StrategyStatus } from '@/types/domain'
import { decimalStringToDisplayNumber } from '@/utils/decimal'
import {
  directionTextMap,
  directionToneMap,
  statusTextMap,
  statusToneMap,
} from '@/utils/labels'

const decimalFormatter = new Intl.NumberFormat('zh-CN', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
})

const moneyFormatters = new Map<number, Intl.NumberFormat>()
const percentFormatters = new Map<number, Intl.NumberFormat>()

/** 按小数位数缓存金额 formatter，候选当前价格可按合约价格步长展示为整数。 */
function moneyFormatter(digits: number) {
  const cached = moneyFormatters.get(digits)
  if (cached) {
    return cached
  }
  const formatter = new Intl.NumberFormat('zh-CN', {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  })
  moneyFormatters.set(digits, formatter)
  return formatter
}

/** 按小数位数缓存百分比 formatter，避免组件重复创建 Intl 对象。 */
function percentFormatter(digits: number) {
  const cached = percentFormatters.get(digits)
  if (cached) {
    return cached
  }
  const formatter = new Intl.NumberFormat('zh-CN', {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  })
  percentFormatters.set(digits, formatter)
  return formatter
}

/** 返回组件常用的格式化函数和状态映射。 */
export function useFormat() {
  // 这里的转换只用于 Intl 展示格式化；不要把返回值再作为计算输入。
  const money = (value: string | number, currency = 'USDT', digits = 2) =>
    `${moneyFormatter(digits).format(toDisplayNumber(value))} ${currency}`

  const decimal = (value: string | number) =>
    decimalFormatter.format(toDisplayNumber(value))

  const percent = (value: string | number, digits = 2) =>
    `${percentFormatter(digits).format(toDisplayNumber(value))}%`

  const signedPercent = (value: string | number) => {
    const numeric = toDisplayNumber(value)
    if (!Number.isFinite(numeric)) {
      return '--'
    }
    return `${numeric > 0 ? '+' : ''}${percent(numeric)}`
  }

  const directionText = (direction: StrategyDirection) => directionTextMap[direction]

  const directionTone = (direction: StrategyDirection) => directionToneMap[direction]

  const statusText = (status: StrategyStatus) => statusTextMap[status]

  const statusTone = (status: StrategyStatus) => statusToneMap[status]

  return {
    decimal,
    money,
    percent,
    signedPercent,
    directionText,
    directionTone,
    statusText,
    statusTone,
  }
}

function toDisplayNumber(value: string | number) {
  return typeof value === 'number' ? value : decimalStringToDisplayNumber(value)
}
