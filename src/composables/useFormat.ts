import type { StrategyDirection, StrategyStatus } from '@/types/domain'

const moneyFormatter = new Intl.NumberFormat('zh-CN', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
})

const decimalFormatter = new Intl.NumberFormat('zh-CN', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
})

const percentFormatters = new Map<number, Intl.NumberFormat>()

const directionTextMap: Record<StrategyDirection, string> = {
  sellPut: '卖 Put 抄底',
  sellCall: '卖 Call 止盈',
}

const directionToneMap: Record<StrategyDirection, 'success' | 'error'> = {
  sellPut: 'success',
  sellCall: 'error',
}

const statusTextMap: Record<StrategyStatus, string> = {
  pending: '待成交',
  running: '进行中',
  settled: '已结算',
  cancelled: '已取消',
  rejected: '已拒绝',
  expired: '已过期',
  partiallyFilled: '部分成交',
  rebalanceFailed: '补齐失败',
}

const statusToneMap: Record<StrategyStatus, 'default' | 'error' | 'info' | 'success' | 'warning'> = {
  pending: 'warning',
  running: 'success',
  settled: 'info',
  cancelled: 'default',
  rejected: 'error',
  expired: 'default',
  partiallyFilled: 'warning',
  rebalanceFailed: 'error',
}

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

export function useFormat() {
  const money = (value: string | number, currency = 'USDT') =>
    `${moneyFormatter.format(Number(value))} ${currency}`

  const decimal = (value: string | number) =>
    decimalFormatter.format(Number(value))

  const percent = (value: string | number, digits = 2) =>
    `${percentFormatter(digits).format(Number(value))}%`

  const directionText = (direction: StrategyDirection) => directionTextMap[direction]

  const directionTone = (direction: StrategyDirection) => directionToneMap[direction]

  const statusText = (status: StrategyStatus) => statusTextMap[status]

  const statusTone = (status: StrategyStatus) => statusToneMap[status]

  return {
    decimal,
    money,
    percent,
    directionText,
    directionTone,
    statusText,
    statusTone,
  }
}
