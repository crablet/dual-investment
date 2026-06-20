// 组件展示类型，不通过 IPC 传输。

import type { LocalizedMessage } from '@/i18n'
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
  id: 'portfolio' | 'margin' | 'premium'
  title: string
  value: string
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
  label: LocalizedMessage
  status: OrderProgressStepStatus
  message?: LocalizedMessage
}

/** 手动下单进度展示模型。 */
export interface OrderProgressView {
  flowId: string
  status: OrderProgressStatus
  steps: OrderProgressStepView[]
  optionSymbol?: string
  exchangeOrderId?: string
  order?: StrategyOrder
  error?: LocalizedMessage
}

export interface OrderProgressNotice {
  type: 'success' | 'error'
  message: LocalizedMessage
}
