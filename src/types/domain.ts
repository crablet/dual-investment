// 对应 src-tauri/src/domain.rs 的 IPC 类型。金融数值使用字符串，避免 JavaScript number 丢失精度。

import type { LocalizedMessage } from '@/i18n'

/** Decimal 通过 IPC 以字符串传输，避免金融精度被 JS number 截断。 */
export type DecimalString = string

/** 双币赢策略方向，来自 Rust `StrategyDirection` 的 camelCase 序列化。 */
export type StrategyDirection = 'sellPut' | 'sellCall'
/** 本机订单的交易所状态，不代表当前持仓或交割进度。 */
export type StrategyStatus
  = | 'pending'
    | 'filled'
    | 'cancelled'
    | 'rejected'
    | 'expired'
    | 'partiallyFilled'
/** 文件日志等级；日志内容必须是后端脱敏摘要。 */
export type LogLevel = 'info' | 'warn' | 'error'
/** 手动下单进度阶段，和 Rust 发出的脱敏进度事件保持一致。 */
export type OrderProgressStage = 'validate' | 'funding' | 'risk' | 'submit' | 'persist'
/** 手动下单进度状态，仅用于 UI 反馈，不作为订单事实。 */
export type OrderProgressStatus = 'running' | 'success' | 'error'
/** 订单与现货补齐恢复记录的持久化状态。 */
export type OrderAttemptStatus
  = | 'attempted'
    | 'exchange_ack'
    | 'persisted'
    | 'failed'
    | 'persist_failed'
    | 'spot_attempted'
    | 'spot_persisted'
/** 真实资金动作恢复记录的持久化状态。 */
export type FundingAttemptStatus
  = | 'attempted'
    | 'exchange_ack'
    | 'uncertain'
    | 'pending'
    | 'confirmed'
    | 'failed'
    | 'blocked'

/** 前端可见运行设置，不包含明文 API Secret。 */
export interface AppSettings {
  connected: boolean
  savedCredentials: boolean
}

/** 后端提供重连资格，前端不从错误文案推断业务类别。 */
export interface CommandErrorPayload {
  message: LocalizedMessage
  canAutoReconnect: boolean
}

/** 账户资产展示行，可能来自期权保证金、现货或 Simple Earn。 */
export interface BalanceRow {
  asset: string
  source: string
  free: DecimalString
  locked: DecimalString
  usdtValue: DecimalString
  usdtValueKnown: boolean
}

/** Rust 按资产聚合的余额，明细保留真实账户来源。 */
export interface AssetBalance {
  asset: string
  free: DecimalString
  locked: DecimalString
  usdtValue: DecimalString
  usdtValueKnown: boolean
  details: BalanceRow[]
}

/** 最后一次成功账户查询的缓存；失败保留旧数据和独立错误。 */
export interface AccountSnapshot {
  balances: AssetBalance[]
  totalUsdtValue: DecimalString
  availableMargin: DecimalString
  unpricedAssetCount: number
  updatedAt: string | null
  error: LocalizedMessage | null
}

/** Binance 实盘合约、买一价和 mark greeks 派生出的候选机会。 */
export interface StrategyCandidate {
  symbol: string
  optionSymbol: string
  direction: StrategyDirection
  expiryAt: string
  strike: DecimalString
  quantity: DecimalString
  limitPrice: DecimalString
  annualized: DecimalString
  exerciseProbability: DecimalString
  spotPrice: DecimalString
  strikeDistancePercent: DecimalString
}

/** 本地保存的真实策略订单。 */
export interface StrategyOrder {
  id: string
  exchangeOrderId: string | null
  clientOrderId: string
  symbol: string
  optionSymbol: string
  direction: StrategyDirection
  status: StrategyStatus
  expiryAt: string
  strike: DecimalString
  unit: DecimalString
  quantity: DecimalString
  filledQuantity: DecimalString
  avgPrice: DecimalString
  premium: DecimalString
  annualized: DecimalString
  breakeven: DecimalString
  exchangeStatus: string
  createdAt: string
  updatedAt: string
}

export type PositionStatus = 'open' | 'closed' | 'expired'

/** 完整合约代码对应账户的卖方净仓，数量以合约张数表示。 */
export interface TrackedPosition {
  optionSymbol: string
  symbol: string
  direction: StrategyDirection
  status: PositionStatus
  expiryAt: string
  strike: DecimalString
  unit: DecimalString
  quantity: DecimalString
  entryPrice: DecimalString
  markPrice: DecimalString
  unrealizedPnl: DecimalString
  trackedAt: string
  lastSyncedAt: string
}

export interface PositionSyncStatus {
  updatedAt: string | null
  error: LocalizedMessage | null
}

export type DeliveryStatus = 'waitingExercise' | 'ready' | 'executing' | 'completed' | 'failed'

/** 仅为到期前已登记净仓生成的交割任务；行权事实未确认时保留 null。 */
export interface DeliveryTask {
  id: string
  symbol: string
  optionSymbol: string
  direction: StrategyDirection
  status: DeliveryStatus
  expiryAt: string
  trackedAt: string
  strike: DecimalString
  unit: DecimalString
  lastPositionQuantity: DecimalString
  quantity: DecimalString | null
  settlementPrice: DecimalString | null
  cashSettlement: DecimalString | null
  exerciseRecordIds: string[]
  spotOrderId: string | null
  spotClientOrderId: string
  spotOrderStatus: string
  spotFilledQuantity: DecimalString
  spotAvgPrice: DecimalString
  message: LocalizedMessage
  createdAt: string
  updatedAt: string
  completedAt: string | null
}

/** 本机卖出权利金汇总，不代表账户累计收益。 */
export interface PnlReport {
  recordedPremium: DecimalString
  todayRecordedPremium: DecimalString
  sellPutPremium: DecimalString
  sellCallPremium: DecimalString
}

/** 已成交且仍存续的义务仓汇总；名义本金不代表真实保证金占用。 */
export interface PortfolioSummary {
  putCount: number
  callCount: number
  putQuantity: DecimalString
  putNotional: DecimalString
  callQuantity: DecimalString
}

/** 文件日志记录，只展示后端写入的脱敏摘要。 */
export interface LogEntry {
  id: string
  timestamp: string
  level: LogLevel
  event: string
  message: LocalizedMessage
}

/** 设置页日志状态快照，只包含脱敏日志摘要和最小文件状态。 */
export interface LogSnapshot {
  hasFiles: boolean
  recentEntries: LogEntry[]
  writeError: LocalizedMessage | null
}

/** 下单/补齐恢复记录视图。 */
export interface OrderAttempt {
  id: string
  clientOrderId: string
  symbol: string
  optionSymbol: string
  status: OrderAttemptStatus
  exchangeOrderId: string | null
  message: LocalizedMessage
  updatedAt: string
}

/** 资金动作恢复记录视图。 */
export interface FundingAttemptView {
  id: string
  kindLabel: LocalizedMessage
  asset: string
  amount: DecimalString
  transferType: string | null
  productId: string | null
  exchangeRef: string | null
  status: FundingAttemptStatus
  message: LocalizedMessage
  factSource: LocalizedMessage
  nextAction: LocalizedMessage
  queryAfterAt: string | null
  queryCount: number
  effectiveQueryCount: number
  transportErrorCount: number
  submitErrorKind: string | null
  historyErrorKind: string | null
  historyWindowStartAt: string | null
  historyWindowEndAt: string | null
  updatedAt: string
}

export type FundingAttemptManualResolution = 'confirmed' | 'failed'

export type SettlementStage
  = | 'waitingExercise'
    | 'waitingFunds'
    | 'confirmingFunds'
    | 'executing'
    | 'confirmingOrders'
    | 'pendingSweep'
    | 'blocked'
    | 'completed'
    | 'sweepSkipped'

/** 批次恢复记录，操作资格和原因均来自 Rust。 */
export interface SettlementRecoveryView {
  id: string
  symbol: string
  expiryAt: string
  status: SettlementStage
  statusLabel: LocalizedMessage
  canFinish: boolean
  factSource: LocalizedMessage
  nextAction: LocalizedMessage
  message: LocalizedMessage
  updatedAt: string
}

export interface FinishSettlementSweepRequest {
  batchId: string
  confirmManual: boolean
}

/** 人工核对资金恢复记录后的处置请求。 */
export interface ResolveFundingAttemptRequest {
  attemptId: string
  resolution: FundingAttemptManualResolution
  confirmManual: boolean
}

/** 手动下单进度事件，只用于前端反馈，不作为交易事实来源。 */
export interface OrderProgressEvent {
  flowId: string
  stage: OrderProgressStage
  status: OrderProgressStatus
  message: LocalizedMessage
  optionSymbol: string | null
  exchangeOrderId: string | null
}

/** 前端一次刷新拿到的完整应用快照。 */
export interface AppSnapshot {
  settings: AppSettings
  account: AccountSnapshot
  candidates: StrategyCandidate[]
  orders: StrategyOrder[]
  positions: TrackedPosition[]
  deliveries: DeliveryTask[]
  positionSync: PositionSyncStatus
  pnl: PnlReport
  portfolio: PortfolioSummary
  orderAttempts: OrderAttempt[]
  fundingAttempts: FundingAttemptView[]
  settlementRecoveries: SettlementRecoveryView[]
}

/** 连接 Binance 的表单请求。Secret 只会传给 Rust keyring 写入流程。 */
export interface ConnectBinanceRequest {
  apiKey: string
  apiSecret: string
}

/** 用户确认的合约参数；必填字段缺失时拒绝请求，confirmLive 表示已确认实盘下单。 */
export interface StrategyRequest {
  clientFlowId: string
  symbol: string
  optionSymbol: string
  direction: StrategyDirection
  expiryAt: string
  strike: DecimalString
  quantity: DecimalString
  limitPrice: DecimalString
  confirmLive: boolean
}

/** 纯本地 Decimal 预估，不读取交易所或创建订单。 */
export interface StrategyPreviewRequest {
  direction: StrategyDirection
  strike: DecimalString
  limitPrice: DecimalString
  quantity: DecimalString
}

export interface StrategyPreview {
  premium: DecimalString
  breakeven: DecimalString
}
