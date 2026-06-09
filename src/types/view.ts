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

/** 前端步骤在收到后端事件前可以处于等待态；IPC 事件本身没有 `pending`。 */
export type OrderProgressStepStatus = OrderProgressStatus | 'pending'

/** 总览指标卡展示模型。 */
export interface MetricCardModel {
  id: string
  title: string
  value: string
  icon: 'wallet' | 'shield' | 'coins' | 'activity'
  tone: 'gold' | 'blue' | 'gray'
}

/** 构成图展示项。 */
export interface PnlBreakdownItem {
  /** 稳定业务 key 用于固定颜色和图形排序。 */
  key: string
  label: string
  value: DecimalString
}

/** 轻量条形图展示项。 */
export interface BarListItem {
  label: string
  value: string
  width: string
}

/** 手动下单进度步骤展示模型。 */
export interface OrderProgressStepView {
  id: OrderProgressStage
  label: string
  status: OrderProgressStepStatus
  message?: string
}

/** 手动下单进度展示模型。 */
export interface OrderProgressView {
  flowId: string
  status: OrderProgressStatus
  steps: OrderProgressStepView[]
  optionSymbol?: string
  exchangeOrderId?: string
  order?: StrategyOrder
  error?: string
}

export interface OrderProgressNotice {
  type: 'success' | 'error'
  message: string
}
