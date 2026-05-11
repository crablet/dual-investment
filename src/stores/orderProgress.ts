// 手动下单进度视图模型构造器。
//
// Rust 通过脱敏事件上报当前阶段，Pinia store 用这些纯函数合成右侧下单进度面板。
// 进度只服务交互反馈，不作为订单事实来源；订单事实仍以后端快照和恢复记录为准。

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

/** 每个阶段成功时展示的默认摘要；后端事件可用更具体的脱敏消息覆盖。 */
export const orderProgressSuccessMessages: Record<OrderProgressStage, string> = {
  validate: '下单参数已通过校验',
  funding: '资金准备已完成',
  risk: '本地下单风控已通过',
  submit: 'Binance 已返回订单状态',
  persist: '订单已保存',
}

/** 创建一次手动下单的初始进度；此时真实订单还没有发送到 Binance。 */
export function createInitialOrderProgress(
  flowId: string,
  request: StrategyRequest,
  now = new Date().toISOString(),
): OrderProgressView {
  return {
    flowId,
    status: 'running',
    optionSymbol: request.optionSymbol,
    startedAt: now,
    updatedAt: now,
    steps: createOrderProgressSteps('validate', 'running', '正在提交下单请求', now),
  }
}

/**
 * 从已创建订单重建可展示的下单请求。
 *
 * 只用于前端把成功结果关联回进度面板，不会重新发送真实订单。
 */
export function createOrderProgressFromOrder(flowId: string, order: StrategyOrder): StrategyRequest {
  return {
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

/** 构造五阶段进度列表，只有当前阶段带状态、消息和时间戳。 */
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

/** 从步骤状态汇总整个进度面板状态，错误优先级最高。 */
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

/**
 * 把当前进行中的步骤标记为错误。
 *
 * 如果还没收到运行中阶段，就落到第一个 pending 步骤；这样网络或 IPC 早期失败也能
 * 在进度条上有明确落点，而不是只显示全局错误。
 */
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

/** 生成前端流程 ID，用于把本次提交、进度事件和成功订单临时关联起来。 */
export function makeOrderFlowId() {
  return `manual-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}
