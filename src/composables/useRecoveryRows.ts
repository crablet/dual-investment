// 订单页恢复中心的展示模型和操作分派。
//
// 订单恢复与资金恢复共用一张表只服务扫描效率；真实恢复语义仍由不同 Rust command
// 决定，前端不会把 blocked 资金记录或订单记录直接改成业务事实。

import type {
  FundingAttemptManualResolution,
  FundingAttemptStatus,
  FundingAttemptView,
  OrderAttempt,
  OrderAttemptStatus,
  SettlementRecoveryView,
  SettlementStage,
} from '@/types/domain'
import { computed } from 'vue'
import { messageText, t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import { localDateTimeLabel } from '@/utils/date'
import { attemptStatusText, recoveryStatusTone as attemptStatusTone, fundingAttemptStatusText } from '@/utils/labels'

interface RecoveryRowBase {
  id: string
  updatedAt: string
  type: string
  target: string
  amount: string
  exchangeRef: string
  factSource: string
  nextAction: string
  message: string
  diagnostics: string
}

/** 区分订单与资金恢复状态，点击操作时必须分派到对应 Rust command。 */
export type RecoveryRow
  = | RecoveryRowBase & { source: 'order', status: OrderAttemptStatus }
    | RecoveryRowBase & { source: 'funding', status: FundingAttemptStatus }
    | RecoveryRowBase & { source: 'settlement', status: SettlementStage, statusLabel: string, canFinish: boolean }

/** 组合订单页恢复中心的列表模型和行级操作。 */
export function useRecoveryRows() {
  const store = useAppStore()

  // 阻断批次不能被最近十条历史挤掉，否则用户会失去处理入口。
  const recoveryRows = computed(() => [
    ...store.settlementRecoveries.map(settlementRecoveryRow),
    ...store.orderAttempts.map(orderAttemptRecoveryRow),
    ...store.fundingAttempts.map(fundingAttemptRecoveryRow),
  ].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)).filter((row, index) =>
    index < 10 || (row.source === 'settlement' && row.status === 'blocked'),
  ))

  function recoveryStatusLabel(row: RecoveryRow) {
    if (row.source === 'settlement') {
      return row.statusLabel
    }
    return row.source === 'funding'
      ? fundingAttemptStatusText(row.status)
      : attemptStatusText(row.status)
  }

  function recoveryStatusTone(row: RecoveryRow) {
    if (row.source === 'settlement') {
      return row.status === 'sweepSkipped' ? 'info' : 'warning'
    }
    return attemptStatusTone(row.status)
  }

  function canRecover(row: RecoveryRow) {
    // 终态记录只展示事实，不再触发恢复；blocked 资金记录仍允许“重查”。
    return row.source !== 'settlement'
      && !['persisted', 'spot_persisted', 'confirmed', 'failed'].includes(row.status)
  }

  function canManuallyResolve(row: RecoveryRow) {
    // 只有 blocked 资金动作允许人工解除阻断，订单恢复不能被前端直接改终态。
    return row.source === 'funding' && row.status === 'blocked'
  }

  function recoverActionLabel(row: RecoveryRow) {
    return row.status === 'blocked' ? t('ui.queryAgain') : t('ui.recovery')
  }

  async function recoverRow(row: RecoveryRow) {
    if (row.source === 'order') {
      await store.recoverAttempt(row.id)
      return
    }
    if (row.source === 'funding') {
      await store.recoverFundingAttempt(row.id)
    }
  }

  async function resolveFundingRow(row: RecoveryRow, resolution: FundingAttemptManualResolution) {
    const label = resolution === 'confirmed'
      ? t('ui.verifiedOnBinanceThatThisFundingAction')
      : t('ui.verifiedOnBinanceThatThisFundingAction2')
    if (!window.confirm(t('ui.confirmYouHaveThisOnlyRemovesThe', { p0: label }))) {
      return
    }
    await store.resolveFundingAttempt(row.id, resolution)
  }

  async function finishSettlementRow(row: RecoveryRow) {
    if (row.source !== 'settlement' || !row.canFinish) {
      return
    }
    if (!window.confirm(t('ui.endEarnFinalizationForUnsubscribedFundsStay', { p0: row.target }))) {
      return
    }
    await store.finishSettlementSweep(row.id)
  }

  return {
    recoveryRows,
    recoveryStatusLabel,
    recoveryStatusTone,
    canRecover,
    canManuallyResolve,
    recoverActionLabel,
    recoverRow,
    resolveFundingRow,
    finishSettlementRow,
  }
}

function settlementRecoveryRow(recovery: SettlementRecoveryView): RecoveryRow {
  return {
    id: recovery.id,
    source: 'settlement',
    updatedAt: recovery.updatedAt,
    type: t('ui.settlementFinalization'),
    target: `${recovery.symbol} · ${localDateTimeLabel(recovery.expiryAt)}`,
    amount: recovery.symbol,
    exchangeRef: '--',
    status: recovery.status,
    statusLabel: messageText(recovery.statusLabel),
    canFinish: recovery.canFinish,
    factSource: messageText(recovery.factSource),
    nextAction: messageText(recovery.nextAction),
    message: messageText(recovery.message),
    diagnostics: '',
  }
}

function orderAttemptRecoveryRow(attempt: OrderAttempt): RecoveryRow {
  return {
    id: attempt.id,
    source: 'order',
    updatedAt: attempt.updatedAt,
    type: attempt.status.startsWith('spot_') ? t('ui.spotRebalancing') : t('ui.optionOrder'),
    target: attempt.optionSymbol,
    amount: attempt.symbol,
    exchangeRef: attempt.exchangeOrderId ?? attempt.clientOrderId,
    status: attempt.status,
    factSource: attempt.exchangeOrderId
      ? t('ui.binanceOrder', { p0: attempt.exchangeOrderId })
      : t('ui.clientOrder', { p0: attempt.clientOrderId }),
    nextAction: attempt.status === 'persisted' || attempt.status === 'spot_persisted'
      ? t('ui.noActionNeeded')
      : t('ui.queryBinanceOrderAgain'),
    message: messageText(attempt.message),
    diagnostics: '',
  }
}

function fundingAttemptRecoveryRow(attempt: FundingAttemptView): RecoveryRow {
  // 资金恢复行直接展示后端给出的 factSource/nextAction，前端不推断 Binance 历史结果。
  return {
    id: attempt.id,
    source: 'funding',
    updatedAt: attempt.updatedAt,
    type: messageText(attempt.kindLabel),
    target: attempt.transferType ?? attempt.productId ?? attempt.asset,
    amount: `${attempt.amount} ${attempt.asset}`,
    exchangeRef: attempt.exchangeRef ?? '--',
    status: attempt.status,
    factSource: messageText(attempt.factSource),
    nextAction: messageText(attempt.nextAction),
    message: messageText(attempt.message),
    diagnostics: fundingAttemptDiagnostics(attempt),
  }
}

function fundingAttemptDiagnostics(attempt: FundingAttemptView) {
  const parts = [
    t('ui.queries', { p0: attempt.queryCount }),
    t('ui.effectiveQueries', { p0: attempt.effectiveQueryCount }),
    t('ui.temporaryErrors', { p0: attempt.transportErrorCount }),
  ]
  if (attempt.queryAfterAt) {
    parts.push(t('ui.next', { p0: localDateTimeLabel(attempt.queryAfterAt) }))
  }
  if (attempt.submitErrorKind) {
    parts.push(t('ui.submissionError', { p0: attempt.submitErrorKind }))
  }
  if (attempt.historyErrorKind) {
    parts.push(t('ui.historyError', { p0: attempt.historyErrorKind }))
  }
  if (attempt.historyWindowStartAt && attempt.historyWindowEndAt) {
    parts.push(
      t('ui.window', { p0: localDateTimeLabel(attempt.historyWindowStartAt), p1: localDateTimeLabel(attempt.historyWindowEndAt) }),
    )
  }
  return parts.join(' · ')
}
