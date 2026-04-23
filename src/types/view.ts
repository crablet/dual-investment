// 前端派生视图模型。
//
// 这些类型不跨 IPC，只服务于组件展示。真实业务事实仍以 `types/domain.ts` 中的
// Rust 快照类型为准。

import type { DecimalString } from '@/types/domain'

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

/** 收益构成图展示项。 */
export interface PnlBreakdownItem {
  label: string
  value: DecimalString
  color: string
}
