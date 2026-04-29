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
  /** 稳定业务 key 用于固定颜色和图形排序；缺省时组件会退回到 label。 */
  key?: string
  label: string
  value: DecimalString
  /** 可由调用方指定固定色，避免刷新后图表色彩漂移。 */
  color?: string
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
