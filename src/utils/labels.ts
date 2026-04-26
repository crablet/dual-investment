// 前端展示文案映射。
//
// 这里只维护 UI 标签和组件 tone。Rust 侧仍保留日志与错误提示所需的业务文案。

import type { LogLevel, StrategyDirection, StrategyStatus } from '@/types/domain'

export const directionTextMap: Record<StrategyDirection, string> = {
  sellPut: '卖 Put 抄底',
  sellCall: '卖 Call 止盈',
}

export const directionToneMap: Record<StrategyDirection, 'success' | 'error'> = {
  sellPut: 'success',
  sellCall: 'error',
}

export const statusTextMap: Record<StrategyStatus, string> = {
  pending: '待成交',
  running: '进行中',
  settled: '已结算',
  cancelled: '已取消',
  rejected: '已拒绝',
  expired: '已过期',
  partiallyFilled: '部分成交',
  rebalanceFailed: '补齐失败',
}

export const statusToneMap: Record<StrategyStatus, 'default' | 'error' | 'info' | 'success' | 'warning'> = {
  pending: 'warning',
  running: 'success',
  settled: 'info',
  cancelled: 'default',
  rejected: 'error',
  expired: 'default',
  partiallyFilled: 'warning',
  rebalanceFailed: 'error',
}

export const attemptStatusTextMap: Record<string, string> = {
  attempted: '准备发送期权订单',
  exchange_ack: '交易所已接单',
  failed: '下单失败',
  persist_failed: '订单保存失败',
  spot_attempted: '准备发送现货补齐',
  spot_exchange_ack: '现货交易所已接单',
  spot_persist_failed: '现货补齐保存失败',
}

export const logLevelToneMap: Record<LogLevel, 'default' | 'error' | 'info' | 'success' | 'warning'> = {
  info: 'info',
  warn: 'warning',
  error: 'error',
}

/** 策略状态中文文案。 */
export function strategyStatusText(status: StrategyStatus) {
  return statusTextMap[status]
}

/** 恢复记录状态中文文案。 */
export function attemptStatusText(status: string) {
  return attemptStatusTextMap[status] ?? status
}

/** 日志等级标签 tone。 */
export function logLevelTone(level: LogLevel) {
  return logLevelToneMap[level]
}
