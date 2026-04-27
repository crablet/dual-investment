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
  persisted: '订单已保存',
  failed: '下单失败',
  persist_failed: '订单保存失败',
  spot_attempted: '准备发送现货补齐',
  spot_exchange_ack: '现货交易所已接单',
  spot_persisted: '现货补齐已保存',
  spot_persist_failed: '现货补齐保存失败',
  uncertain: '结果待确认',
  pending: '处理中',
  confirmed: '已确认',
  blocked: '已阻断',
}

export const recoveryStatusToneMap: Record<string, 'default' | 'error' | 'info' | 'success' | 'warning'> = {
  attempted: 'warning',
  exchange_ack: 'info',
  persisted: 'success',
  failed: 'error',
  persist_failed: 'error',
  spot_attempted: 'warning',
  spot_exchange_ack: 'info',
  spot_persisted: 'success',
  spot_persist_failed: 'error',
  uncertain: 'warning',
  pending: 'info',
  confirmed: 'success',
  blocked: 'error',
}

export const fundingAttemptStatusTextMap: Record<string, string> = {
  attempted: '准备发送资金请求',
  exchange_ack: '交易所已返回引用',
  uncertain: '结果待确认',
  pending: '处理中',
  confirmed: '已确认',
  failed: '资金动作失败',
  blocked: '已阻断',
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

/** 恢复记录状态标签 tone。 */
export function recoveryStatusTone(status: string) {
  return recoveryStatusToneMap[status] ?? 'default'
}

/** 资金恢复记录状态中文文案。 */
export function fundingAttemptStatusText(status: string) {
  return fundingAttemptStatusTextMap[status] ?? attemptStatusText(status)
}

/** 日志等级标签 tone。 */
export function logLevelTone(level: LogLevel) {
  return logLevelToneMap[level]
}
