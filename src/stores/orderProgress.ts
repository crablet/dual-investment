// 手动下单进度视图模型构造器。
//
// Rust 通过脱敏事件上报当前阶段，Pinia store 用这些纯函数合成右侧下单进度面板。
// 进度只服务交互反馈，不作为订单事实来源；订单事实仍以后端快照和恢复记录为准。

import type { LocalizedMessage } from '@/i18n'
import type {
  OrderProgressStage,
  OrderProgressStatus,
} from '@/types/domain'
import type {
  OrderProgressStepStatus,
  OrderProgressStepView,
  OrderProgressView,
} from '@/types/view'
import { message as localized } from '@/i18n'

const orderProgressStages: Array<{ id: OrderProgressStage, label: LocalizedMessage }> = [
  { id: 'validate', label: localized('ui.validation') },
  { id: 'funding', label: localized('ui.funding') },
  { id: 'risk', label: localized('ui.riskChecks') },
  { id: 'submit', label: localized('ui.submitToBinance') },
  { id: 'persist', label: localized('ui.saveOrder') },
]

/** 每个阶段成功时展示的默认摘要；后端事件可用更具体的脱敏消息覆盖。 */
export const orderProgressSuccessMessages: Record<OrderProgressStage, LocalizedMessage> = {
  validate: localized('ui.orderParametersValidated'),
  funding: localized('ui.fundingReady'),
  risk: localized('ui.localPreTradeChecksPassed'),
  submit: localized('ui.binanceReturnedTheOrderStatus'),
  persist: localized('ui.orderSaved'),
}

/** 创建一次手动下单的初始进度；此时真实订单还没有发送到 Binance。 */
export function createInitialOrderProgress(
  flowId: string,
  optionSymbol: string,
): OrderProgressView {
  return {
    flowId,
    status: 'running',
    optionSymbol,
    steps: createOrderProgressSteps('validate', 'running', localized('ui.submittingOrderRequest')),
  }
}

/** 构造五阶段进度列表，只有当前阶段带状态和消息。 */
export function createOrderProgressSteps(
  activeStage?: OrderProgressStage,
  status: OrderProgressStepStatus = 'pending',
  message?: LocalizedMessage,
): OrderProgressStepView[] {
  return orderProgressStages.map(stage => ({
    id: stage.id,
    label: stage.label,
    status: stage.id === activeStage ? status : 'pending',
    message: stage.id === activeStage ? message : undefined,
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
  return 'running'
}

/**
 * 把当前进行中的步骤标记为错误。
 *
 * 如果还没收到运行中阶段，就落到第一个 pending 步骤；这样网络或 IPC 早期失败也能
 * 在进度条上有明确落点，而不是只显示全局错误。
 */
export function markCurrentOrderStepError(
  steps: OrderProgressStepView[],
  message: LocalizedMessage,
) {
  const runningIndex = steps.findIndex(step => step.status === 'running')
  const pendingIndex = steps.findIndex(step => step.status === 'pending')
  const fallbackIndex = Math.max(0, steps.length - 1)
  const targetIndex = runningIndex >= 0 ? runningIndex : pendingIndex >= 0 ? pendingIndex : fallbackIndex
  return steps.map((step, index) =>
    index === targetIndex
      ? { ...step, status: 'error' as OrderProgressStepStatus, message }
      : step,
  )
}

/** 生成前端流程 ID，用于把本次提交、进度事件和成功订单临时关联起来。 */
export function makeOrderFlowId() {
  return `manual-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}
