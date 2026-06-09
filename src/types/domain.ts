// Rust/Tauri IPC 领域类型。
//
// 这些接口镜像 `src-tauri/src/domain.rs` 中序列化到前端的 DTO。金额、价格、数量、
// 权利金和收益率都以字符串承接，前端只在展示层临时格式化，不能把这些字段
// 当作 JavaScript number 再回传为交易事实。

/** Decimal 通过 IPC 以字符串传输，避免金融精度被 JS number 截断。 */
export type DecimalString = string

/** 双币赢策略方向，来自 Rust `StrategyDirection` 的 camelCase 序列化。 */
export type StrategyDirection = 'sellPut' | 'sellCall'
/** 本地订单状态，包含交易所状态归一化和现货补齐失败等本地状态。 */
export type StrategyStatus
  = | 'pending'
    | 'running'
    | 'settled'
    | 'cancelled'
    | 'rejected'
    | 'expired'
    | 'partiallyFilled'
    | 'rebalanceFailed'
/** 文件日志等级；日志内容必须是后端脱敏摘要。 */
export type LogLevel = 'info' | 'warn' | 'error'
/** 手动下单进度阶段，和 Rust 发出的脱敏进度事件保持一致。 */
export type OrderProgressStage = 'validate' | 'funding' | 'risk' | 'submit' | 'persist'
/** 手动下单进度状态，仅用于 UI 反馈，不作为订单事实。 */
export type OrderProgressStatus = 'running' | 'success' | 'error'
/** 本地跟踪订单来源；接管仓位和本机下单不能用订单号是否存在来推断。 */
export type StrategyOrderSource = 'liveOrder' | 'adoptedPosition'
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

/** 账户资产展示行，可能来自期权保证金、现货或 Simple Earn。 */
export interface BalanceRow {
  asset: string
  source: string
  free: DecimalString
  locked: DecimalString
  usdtValue: DecimalString
  usdtValueKnown: boolean
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
  source: StrategyOrderSource
  exchangeOrderId: string | null
  clientOrderId: string
  symbol: string
  optionSymbol: string
  direction: StrategyDirection
  status: StrategyStatus
  expiryAt: string
  strike: DecimalString
  quantity: DecimalString
  filledQuantity: DecimalString
  avgPrice: DecimalString
  premium: DecimalString
  annualized: DecimalString
  breakeven: DecimalString
  exchangeStatus: string
  createdAt: string
  updatedAt: string
  settlementAction: string
  settlementPrice: DecimalString
  settledAt: string | null
  spotOrderId: string | null
  spotClientOrderId: string
  spotOrderStatus: string
  rebalanceMessage: string
  spotFilledQuantity: DecimalString
  spotAvgPrice: DecimalString
}

/** 可由本机接管监控的 Binance 当前期权持仓。 */
export interface AdoptableOptionPositionView {
  symbol: string
  direction: StrategyDirection
  expiryAt: string
  strike: DecimalString
  quantity: DecimalString
  entryPrice: DecimalString
  markPrice: DecimalString
  premium: DecimalString
  alreadyTracked: boolean
  adoptable: boolean
  reason: string
}

/** 接管 Binance 当前期权持仓请求。 */
export interface AdoptOptionPositionsRequest {
  symbols: string[]
  confirmLive: boolean
}

/** 收益报告，图表构成由前端再派生。 */
export interface PnlReport {
  cumulative: DecimalString
  today: DecimalString
  optionPremium: DecimalString
  floatingSpot: DecimalString
}

/** 文件日志记录，只展示后端写入的脱敏摘要。 */
export interface LogEntry {
  id: string
  timestamp: string
  level: LogLevel
  event: string
  message: string
}

/** 设置页日志状态快照，只包含脱敏日志摘要和最小文件状态。 */
export interface LogSnapshot {
  hasFiles: boolean
  recentEntries: LogEntry[]
}

/** 下单/补齐恢复记录视图。 */
export interface OrderAttemptView {
  id: string
  clientOrderId: string
  symbol: string
  optionSymbol: string
  status: OrderAttemptStatus
  exchangeOrderId: string | null
  message: string
  updatedAt: string
}

/** 资金动作恢复记录视图。 */
export interface FundingAttemptView {
  id: string
  kindLabel: string
  asset: string
  amount: DecimalString
  transferType: string | null
  productId: string | null
  exchangeRef: string | null
  status: FundingAttemptStatus
  message: string
  factSource: string
  nextAction: string
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
  statusLabel: string
  canFinish: boolean
  factSource: string
  nextAction: string
  message: string
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
  message: string
  optionSymbol: string | null
  exchangeOrderId: string | null
}

/** 前端一次刷新拿到的完整应用快照。 */
export interface AppSnapshot {
  settings: AppSettings
  balances: BalanceRow[]
  candidates: StrategyCandidate[]
  orders: StrategyOrder[]
  pnl: PnlReport
  orderAttempts: OrderAttemptView[]
  fundingAttempts: FundingAttemptView[]
  settlementRecoveries: SettlementRecoveryView[]
}

/** 连接 Binance 的表单请求。Secret 只会传给 Rust keyring 写入流程。 */
export interface ConnectBinanceRequest {
  apiKey: string
  apiSecret: string
}

/** 携带用户冻结并确认的合约输入；字段不能缺失后回退匹配，`confirmLive` 是实盘门禁。 */
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
