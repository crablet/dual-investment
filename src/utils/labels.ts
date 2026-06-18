// 前端展示文案映射。
//
// 这里只维护 UI 标签和组件 tone。Rust 侧返回日志与错误提示的消息代码和参数。

import type {
  DeliveryStatus,
  FundingAttemptStatus,
  LogLevel,
  OrderAttemptStatus,
  PositionStatus,
  StrategyDirection,
  StrategyStatus,
} from '@/types/domain'
import { t } from '@/i18n'

type RecoveryStatus = FundingAttemptStatus | OrderAttemptStatus

export const directionTextMap: Record<StrategyDirection, string> = {
  get sellPut() { return t('ui.sellPut') },
  get sellCall() { return t('ui.sellCall') },
}

export const directionToneMap: Record<StrategyDirection, 'success' | 'error'> = {
  sellPut: 'success',
  sellCall: 'error',
}

export const statusTextMap: Record<StrategyStatus, string> = {
  get pending() { return t('ui.pendingFill') },
  get filled() { return t('ui.filled') },
  get cancelled() { return t('ui.cancelled') },
  get rejected() { return t('ui.rejected') },
  get expired() { return t('ui.expired') },
  get partiallyFilled() { return t('ui.partiallyFilled') },
}

export const statusToneMap: Record<StrategyStatus, 'default' | 'error' | 'info' | 'success' | 'warning'> = {
  pending: 'warning',
  filled: 'success',
  cancelled: 'default',
  rejected: 'error',
  expired: 'default',
  partiallyFilled: 'warning',
}

const positionStatusMessages: Record<PositionStatus, string> = {
  open: 'ui.positionOpen',
  closed: 'ui.positionClosed',
  expired: 'ui.expired',
}

const deliveryStatusMessages: Record<DeliveryStatus, string> = {
  waitingExercise: 'ui.deliveryWaitingExercise',
  ready: 'ui.deliveryReady',
  executing: 'ui.deliveryExecuting',
  completed: 'ui.deliveryCompleted',
  failed: 'ui.deliveryFailed',
}

const deliveryStatusTones: Record<DeliveryStatus, 'info' | 'success' | 'warning' | 'error'> = {
  waitingExercise: 'warning',
  ready: 'info',
  executing: 'info',
  completed: 'success',
  failed: 'error',
}

export function positionStatusText(status: PositionStatus) {
  return t(positionStatusMessages[status])
}

export function deliveryStatusText(status: DeliveryStatus) {
  return t(deliveryStatusMessages[status])
}

export function deliveryStatusTone(status: DeliveryStatus) {
  return deliveryStatusTones[status]
}

export const attemptStatusTextMap: Record<OrderAttemptStatus, string> = {
  get attempted() { return t('ui.preparingOptionOrder') },
  get exchange_ack() { return t('ui.exchangeAcceptedOrder') },
  get persisted() { return t('ui.orderSaved') },
  get failed() { return t('ui.orderFailed') },
  get persist_failed() { return t('ui.orderSaveFailed') },
  get spot_attempted() { return t('ui.preparingSpotRebalancing') },
  get spot_persisted() { return t('ui.spotRebalancingSaved') },
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
  get attempted() { return t('ui.preparingFundingRequest') },
  get exchange_ack() { return t('ui.exchangeReturnedAReference') },
  get uncertain() { return t('ui.awaitingConfirmation') },
  get pending() { return t('ui.processing') },
  get confirmed() { return t('ui.confirmed') },
  get failed() { return t('ui.fundingActionFailed') },
  get blocked() { return t('ui.blocked') },
}

export const logLevelToneMap: Record<LogLevel, 'default' | 'error' | 'info' | 'success' | 'warning'> = {
  info: 'info',
  warn: 'warning',
  error: 'error',
}

/** 策略状态本地化文案。 */
export function strategyStatusText(status: StrategyStatus) {
  return statusTextMap[status]
}

/** 恢复记录状态本地化文案。 */
export function attemptStatusText(status: OrderAttemptStatus) {
  return attemptStatusTextMap[status]
}

/** 恢复记录状态标签 tone。 */
export function recoveryStatusTone(status: RecoveryStatus) {
  return recoveryStatusToneMap[status]
}

/** 资金恢复记录状态本地化文案。 */
export function fundingAttemptStatusText(status: FundingAttemptStatus) {
  return fundingAttemptStatusTextMap[status]
}

/** 日志等级标签 tone。 */
export function logLevelTone(level: LogLevel) {
  return logLevelToneMap[level]
}
