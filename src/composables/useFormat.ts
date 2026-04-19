import type { StrategyDirection, StrategyStatus } from '@/types/domain'

export function useFormat() {
  const money = (value: string | number, currency = 'USDT') =>
    `${Number(value).toLocaleString('zh-CN', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${currency}`

  const percent = (value: string | number, digits = 2) =>
    `${Number(value).toLocaleString('zh-CN', { minimumFractionDigits: digits, maximumFractionDigits: digits })}%`

  const directionText = (direction: StrategyDirection) =>
    direction === 'sellPut' ? '卖 Put 抄底' : '卖 Call 止盈'

  const directionTone = (direction: StrategyDirection) =>
    direction === 'sellPut' ? 'success' : 'error'

  const statusText = (status: StrategyStatus) => {
    const map: Record<StrategyStatus, string> = {
      pending: '待成交',
      running: '进行中',
      settled: '已结算',
      cancelled: '已取消',
      rejected: '已拒绝',
      expired: '已过期',
      partiallyFilled: '部分成交',
      rebalanceFailed: '补齐失败',
    }
    return map[status]
  }

  const statusTone = (status: StrategyStatus) => {
    const map: Record<StrategyStatus, 'default' | 'error' | 'info' | 'success' | 'warning'> = {
      pending: 'warning',
      running: 'success',
      settled: 'info',
      cancelled: 'default',
      rejected: 'error',
      expired: 'default',
      partiallyFilled: 'warning',
      rebalanceFailed: 'error',
    }
    return map[status]
  }

  return {
    money,
    percent,
    directionText,
    directionTone,
    statusText,
    statusTone,
  }
}
