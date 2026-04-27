// 前端派生视图模型。
//
// 这些类型不跨 IPC，只服务于组件展示。真实业务事实仍以 `types/domain.ts` 中的
// Rust 快照类型为准。

import type {
  DecimalString,
  OrderProgressStage,
  OrderProgressStatus,
  StrategyOrder,
} from '@/types/domain'

/** 总览指标卡展示模型。 */
export interface MetricCardModel {
  id: string
  title: string
  value: string
  subValue: string
  change: string
  icon: string
  tone: string
}

/** 构成图展示项。 */
export interface PnlBreakdownItem {
  key?: string
  label: string
  value: DecimalString
  color?: string
}

/** 手动下单进度步骤展示模型。 */
export interface OrderProgressStepView {
  id: OrderProgressStage
  label: string
  status: OrderProgressStatus
  message?: string
  timestamp?: string
}

/** 手动下单进度展示模型。 */
export interface OrderProgressView {
  flowId: string
  source: string
  status: OrderProgressStatus
  steps: OrderProgressStepView[]
  optionSymbol?: string
  exchangeOrderId?: string
  order?: StrategyOrder
  error?: string
  startedAt: string
  updatedAt: string
}

export interface OrderProgressNotice {
  id: string
  type: 'success' | 'error'
  message: string
}
