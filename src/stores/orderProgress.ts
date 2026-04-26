import type {
  OrderProgressStage,
  OrderProgressStatus,
  StrategyOrder,
  StrategyRequest,
} from '@/types/domain'
import type { OrderProgressStepView, OrderProgressView } from '@/types/view'

const orderProgressStages: Array<{ id: OrderProgressStage, label: string }> = [
  { id: 'validate', label: '校验' },
  { id: 'funding', label: '资金准备' },
  { id: 'risk', label: '风控' },
  { id: 'submit', label: '发送 Binance' },
  { id: 'persist', label: '保存订单' },
]

export const orderProgressSuccessMessages: Record<OrderProgressStage, string> = {
  validate: '下单参数已通过校验',
  funding: '资金准备已完成',
  risk: '本地下单风控已通过',
  submit: 'Binance 已返回订单状态',
  persist: '订单已保存',
}

export function createInitialOrderProgress(
  flowId: string,
  request: StrategyRequest,
  now = new Date().toISOString(),
): OrderProgressView {
  return {
    flowId,
    source: 'manual',
    status: 'running',
    optionSymbol: request.optionSymbol,
    startedAt: now,
    updatedAt: now,
    steps: createOrderProgressSteps('validate', 'running', '正在提交下单请求', now),
  }
}

export function createOrderProgressFromOrder(flowId: string, order: StrategyOrder): StrategyRequest {
  return {
    candidateId: undefined,
    clientFlowId: flowId,
    symbol: order.symbol,
    optionSymbol: order.optionSymbol,
    direction: order.direction,
    expiry: order.expiry,
    expiryAt: order.expiryAt,
    strike: order.strike,
    quantity: order.quantity,
    limitPrice: order.avgPrice,
    confirmLive: true,
  }
}

export function createOrderProgressSteps(
  activeStage?: OrderProgressStage,
  status: OrderProgressStatus = 'pending',
  message?: string,
  timestamp?: string,
): OrderProgressStepView[] {
  return orderProgressStages.map(stage => ({
    id: stage.id,
    label: stage.label,
    status: stage.id === activeStage ? status : 'pending',
    message: stage.id === activeStage ? message : undefined,
    timestamp: stage.id === activeStage ? timestamp : undefined,
  }))
}

export function progressStatusFromSteps(steps: OrderProgressStepView[]): OrderProgressStatus {
  if (steps.some(step => step.status === 'error')) {
    return 'error'
  }
  if (steps.find(step => step.id === 'persist')?.status === 'success') {
    return 'success'
  }
  if (steps.some(step => step.status === 'running' || step.status === 'success')) {
    return 'running'
  }
  return 'pending'
}

export function markCurrentOrderStepError(
  steps: OrderProgressStepView[],
  message: string,
  timestamp: string,
) {
  const runningIndex = steps.findIndex(step => step.status === 'running')
  const pendingIndex = steps.findIndex(step => step.status === 'pending')
  const fallbackIndex = Math.max(0, steps.length - 1)
  const targetIndex = runningIndex >= 0 ? runningIndex : pendingIndex >= 0 ? pendingIndex : fallbackIndex
  return steps.map((step, index) =>
    index === targetIndex
      ? { ...step, status: 'error' as OrderProgressStatus, message, timestamp }
      : step,
  )
}

export function makeOrderFlowId() {
  return `manual-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}
