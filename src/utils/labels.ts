// 前端展示文案映射。
//
// 这里只维护 UI 标签和组件 tone。Rust 侧仍保留日志与错误提示所需的业务文案。

import type {
  FundingAttemptStatus,
  LogLevel,
  OrderAttemptStatus,
  StrategyDirection,
  StrategyStatus,
} from '@/types/domain'

type RecoveryStatus = FundingAttemptStatus | OrderAttemptStatus

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

export const attemptStatusTextMap: Record<OrderAttemptStatus, string> = {
  attempted: '准备发送期权订单',
  exchange_ack: '交易所已接单',
  persisted: '订单已保存',
  failed: '下单失败',
  persist_failed: '订单保存失败',
  spot_attempted: '准备发送现货补齐',
  spot_persisted: '现货补齐已保存',
}

export const recoveryStatusToneMap: Record<RecoveryStatus, 'error' | 'info' | 'success' | 'warning'> = {
  attempted: 'warning',
  exchange_ack: 'info',
  persisted: 'success',
  failed: 'error',
  persist_failed: 'error',
  spot_attempted: 'warning',
  spot_persisted: 'success',
  uncertain: 'warning',
  pending: 'info',
  confirmed: 'success',
  blocked: 'error',
}

export const fundingAttemptStatusTextMap: Record<FundingAttemptStatus, string> = {
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
export function attemptStatusText(status: OrderAttemptStatus) {
  return attemptStatusTextMap[status]
}

/** 恢复记录状态标签 tone。 */
export function recoveryStatusTone(status: RecoveryStatus) {
  return recoveryStatusToneMap[status]
}

/** 资金恢复记录状态中文文案。 */
export function fundingAttemptStatusText(status: FundingAttemptStatus) {
  return fundingAttemptStatusTextMap[status]
}

/** 日志等级标签 tone。 */
export function logLevelTone(level: LogLevel) {
  return logLevelToneMap[level]
}
