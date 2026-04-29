// 订单页恢复中心的展示模型和操作分派。
//
// 订单恢复与资金恢复共用一张表只服务扫描效率；真实恢复语义仍由不同 Rust command
// 决定，前端不会把 blocked 资金记录或订单记录直接改成业务事实。

import type {
  FundingAttemptManualResolution,
  FundingAttemptView,
  OrderAttemptView,
} from '@/types/domain'
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { attemptStatusText, fundingAttemptStatusText } from '@/utils/labels'

export interface RecoveryRow {
  id: string
  source: 'order' | 'funding'
  updatedAt: string
  type: string
  target: string
  amount: string
  exchangeRef: string
  status: string
  factSource: string
  nextAction: string
  message: string
}

export function useRecoveryRows() {
  const store = useAppStore()

  const recoveryRows = computed(() => [
    // 两类恢复记录共用列表只是为了页面扫描；执行时仍分派到不同后端 command。
    ...store.orderAttempts.map(orderAttemptRecoveryRow),
    ...store.fundingAttempts.map(fundingAttemptRecoveryRow),
  ].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)).slice(0, 10))

  function recoveryStatusLabel(row: RecoveryRow) {
    return row.source === 'funding'
      ? fundingAttemptStatusText(row.status)
      : attemptStatusText(row.status)
  }

  function canRecover(row: RecoveryRow) {
    // 终态记录只展示事实，不再触发恢复；blocked 资金记录仍允许“重查”。
    return !['persisted', 'spot_persisted', 'confirmed', 'failed'].includes(row.status)
  }

  function canManuallyResolve(row: RecoveryRow) {
    // 只有 blocked 资金动作允许人工解除阻断，订单恢复不能被前端直接改终态。
    return row.source === 'funding' && row.status === 'blocked'
  }

  function recoverActionLabel(row: RecoveryRow) {
    return row.status === 'blocked' ? '重查' : '恢复'
  }

  async function recoverRow(row: RecoveryRow) {
    if (row.source === 'order') {
      await store.recoverAttempt(row.id)
      return
    }
    await store.recoverFundingAttempt(row.id)
  }

  async function resolveFundingRow(row: RecoveryRow, resolution: FundingAttemptManualResolution) {
    const label = resolution === 'confirmed'
      ? '已在 Binance 核实该资金动作成功'
      : '已在 Binance 核实该资金动作失败或未发生'
    if (!window.confirm(`确认${label}？该操作只解除恢复阻断，不会重新发送资金请求。`)) {
      return
    }
    await store.resolveFundingAttempt(row.id, resolution)
  }

  return {
    recoveryRows,
    recoveryStatusLabel,
    canRecover,
    canManuallyResolve,
    recoverActionLabel,
    recoverRow,
    resolveFundingRow,
  }
}

function orderAttemptRecoveryRow(attempt: OrderAttemptView): RecoveryRow {
  return {
    id: attempt.id,
    source: 'order',
    updatedAt: attempt.updatedAt,
    type: attempt.status.startsWith('spot_') ? '现货补齐' : '期权订单',
    target: attempt.optionSymbol,
    amount: attempt.symbol,
    exchangeRef: attempt.exchangeOrderId ?? attempt.clientOrderId,
    status: attempt.status,
    factSource: attempt.exchangeOrderId
      ? `Binance 订单 ${attempt.exchangeOrderId}`
      : `客户端订单 ${attempt.clientOrderId}`,
    nextAction: attempt.status === 'persisted' || attempt.status === 'spot_persisted'
      ? '无需处理'
      : '重新查询 Binance 订单',
    message: attempt.message,
  }
}

function fundingAttemptRecoveryRow(attempt: FundingAttemptView): RecoveryRow {
  // 资金恢复行直接展示后端给出的 factSource/nextAction，前端不推断 Binance 历史结果。
  return {
    id: attempt.id,
    source: 'funding',
    updatedAt: attempt.updatedAt,
    type: attempt.kindLabel,
    target: attempt.transferType ?? attempt.productId ?? attempt.asset,
    amount: `${attempt.amount} ${attempt.asset}`,
    exchangeRef: attempt.exchangeRef ?? '--',
    status: attempt.status,
    factSource: attempt.factSource,
    nextAction: attempt.nextAction,
    message: attempt.message,
  }
}
