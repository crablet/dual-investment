// Rust/Tauri IPC 领域类型。
//
// 这些接口镜像 `src-tauri/src/domain.rs` 中序列化到前端的 DTO。金额、价格、数量、
// 权利金、手续费和收益率都以字符串承接，前端只在展示层临时格式化，不能把这些字段
// 当作 JavaScript number 再回传为交易事实。

/** Decimal 通过 IPC 以字符串传输，避免金融精度被 JS number 截断。 */
export type DecimalString = string

/** 双币赢策略方向，来自 Rust `StrategyDirection` 的 camelCase 序列化。 */
export type StrategyDirection = 'sellPut' | 'sellCall'
/** 本地订单状态，包含交易所状态归一化和现货补齐失败等本地状态。 */
export type StrategyStatus =
  | 'pending'
  | 'running'
  | 'settled'
  | 'cancelled'
  | 'rejected'
  | 'expired'
  | 'partiallyFilled'
  | 'rebalanceFailed'
export type RecommendationLevel = 'recommended' | 'watch' | 'reject'
export type RiskSeverity = 'low' | 'medium' | 'high'

/** 前端可见运行设置，不包含明文 API Secret。 */
export interface AppSettings {
  connected: boolean
  apiKeyMasked?: string
  savedCredentials: boolean
  autoEnabled: boolean
  quickExecution: boolean
  defaultSymbol: string
}

/** 账户资产行，可能来自期权保证金账户或现货账户。 */
export interface BalanceRow {
  asset: string
  source: string
  free: DecimalString
  locked: DecimalString
  usdtValue: DecimalString
  usdtValueKnown: boolean
}

/** Binance 实盘合约和 mark price 派生出的候选机会。 */
export interface StrategyCandidate {
  id: string
  symbol: string
  optionSymbol: string
  direction: StrategyDirection
  expiry: string
  strike: DecimalString
  quantity: DecimalString
  limitPrice: DecimalString
  premium: DecimalString
  annualized: DecimalString
  deltaAbs: DecimalString
  ivRank: DecimalString
  exerciseProbability: DecimalString
  breakeven: DecimalString
  spotPrice: DecimalString
  strikeDistancePercent: DecimalString
  recommendation: RecommendationLevel
  reason: string
}

/** 候选扫描过滤漏斗，用于解释空列表和规则命中情况。 */
export interface CandidateScanDiagnostics {
  rawContracts: number
  rawMarks: number
  btcContracts: number
  tradingContracts: number
  directionMatched: number
  expiryMatched: number
  markMatched: number
  quantityMatched: number
  returnedCandidates: number
  returnedSellPut: number
  returnedSellCall: number
  browseMinDays: number
  browseMaxDays: number
}

export interface CandidateScanResult {
  candidates: StrategyCandidate[]
  diagnostics: CandidateScanDiagnostics
}

/** 本地保存的真实策略订单。 */
export interface StrategyOrder {
  id: string
  exchangeOrderId?: string
  clientOrderId: string
  symbol: string
  optionSymbol: string
  direction: StrategyDirection
  status: StrategyStatus
  expiry: string
  strike: DecimalString
  quantity: DecimalString
  filledQuantity: DecimalString
  avgPrice: DecimalString
  fee: DecimalString
  premium: DecimalString
  estimatedPnl: DecimalString
  breakeven: DecimalString
  exchangeStatus: string
  createdAt: string
  updatedAt: string
  settlementAction: string
  settlementPrice: DecimalString
  settledAt?: string
  spotOrderId?: string
  spotClientOrderId: string
  spotOrderStatus: string
  rebalanceMessage: string
  spotFilledQuantity: DecimalString
  spotAvgPrice: DecimalString
  spotFee: DecimalString
}

/** 自动复投规则；保存规则不代表已经开启自动策略。 */
export interface AutoInvestmentRule {
  enabled: boolean
  schedule: string
  quantity: DecimalString
  minAnnualized: DecimalString
  maxDeltaAbs: DecimalString
  maxIvRank: DecimalString
  minDaysToExpiry: number
  maxDaysToExpiry: number
  includeBtc: boolean
  excludeSameDirection: boolean
}

/** 风控规则集合。 */
export interface RiskRuleSet {
  maxSingleOrderUsdt: DecimalString
  maxDailyOrders: DecimalString
  maxMarginUsage: DecimalString
  maxDrawdown: DecimalString
  stopAfterConsecutiveLosses: number
}

/** 可展示的风险提醒；后端负责保证内容脱敏。 */
export interface RiskAlert {
  id: string
  severity: RiskSeverity
  title: string
  message: string
  createdAt: string
}

/** 当前风险快照。 */
export interface RiskSnapshot {
  score: number
  level: string
  marginUsage: DecimalString
  exerciseRisk: DecimalString
  dailyDrawdown: DecimalString
  rules: RiskRuleSet
  alerts: RiskAlert[]
}

export interface ChartPoint {
  label: string
  value: DecimalString
}

/** 收益报告，图表构成由前端再派生。 */
export interface PnlReport {
  cumulative: DecimalString
  today: DecimalString
  optionPremium: DecimalString
  realizedSpot: DecimalString
  floatingSpot: DecimalString
  fees: DecimalString
  points: ChartPoint[]
}

/** 审计事件只包含脱敏摘要，不应包含 Secret、签名 query 或原始订单响应。 */
export interface AuditEvent {
  id: string
  kind: string
  message: string
  createdAt: string
}

/** 下单/补齐恢复记录视图。 */
export interface OrderAttemptView {
  id: string
  clientOrderId: string
  symbol: string
  optionSymbol: string
  direction: StrategyDirection
  status: string
  exchangeOrderId?: string
  message: string
  createdAt: string
  updatedAt: string
}

/** 前端一次刷新拿到的完整应用快照。 */
export interface AppSnapshot {
  settings: AppSettings
  balances: BalanceRow[]
  candidates: StrategyCandidate[]
  scanDiagnostics: CandidateScanDiagnostics
  orders: StrategyOrder[]
  pnl: PnlReport
  risk: RiskSnapshot
  autoRule: AutoInvestmentRule
  auditRecent: AuditEvent[]
  orderAttempts: OrderAttemptView[]
}

/** 连接 Binance 的表单请求。Secret 只会传给 Rust keyring 写入流程。 */
export interface ConnectBinanceRequest {
  apiKey: string
  apiSecret: string
}

/** 策略下单请求。`confirmLive` 是真实下单门禁。 */
export interface StrategyRequest {
  candidateId?: string
  symbol: string
  optionSymbol?: string
  direction: StrategyDirection
  expiry: string
  strike: DecimalString
  quantity: DecimalString
  limitPrice: DecimalString
  confirmLive: boolean
}

/** 开关自动复投请求；开启时必须经过二次确认。 */
export interface ToggleAutoRequest {
  enabled: boolean
  confirmLive: boolean
}
