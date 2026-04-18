export type DecimalString = string

export type TradingMode = 'simulation' | 'live'
export type StrategyDirection = 'sellPut' | 'sellCall'
export type StrategyStatus = 'pending' | 'running' | 'settled' | 'cancelled' | 'rebalanceFailed'
export type RecommendationLevel = 'recommended' | 'watch' | 'reject'
export type RiskSeverity = 'low' | 'medium' | 'high'

export interface AppSettings {
  mode: TradingMode
  connected: boolean
  apiKeyMasked?: string
  autoEnabled: boolean
  quickExecution: boolean
  defaultSymbol: string
}

export interface MetricCard {
  id: string
  title: string
  value: string
  subValue: string
  change: string
  icon: string
  tone: string
}

export interface BalanceRow {
  asset: string
  free: DecimalString
  locked: DecimalString
  usdtValue: DecimalString
}

export interface StrategyCandidate {
  id: string
  symbol: string
  optionSymbol: string
  direction: StrategyDirection
  expiry: string
  strike: DecimalString
  quantity: DecimalString
  premium: DecimalString
  annualized: DecimalString
  deltaAbs: DecimalString
  ivRank: DecimalString
  exerciseProbability: DecimalString
  breakeven: DecimalString
  recommendation: RecommendationLevel
  reason: string
}

export interface StrategyOrder {
  id: string
  exchangeOrderId?: string
  symbol: string
  optionSymbol: string
  direction: StrategyDirection
  status: StrategyStatus
  expiry: string
  strike: DecimalString
  quantity: DecimalString
  premium: DecimalString
  estimatedPnl: DecimalString
  breakeven: DecimalString
  createdAt: string
  updatedAt: string
  settlementAction: string
}

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

export interface RiskRuleSet {
  maxSingleOrderUsdt: DecimalString
  maxDailyOrders: DecimalString
  maxMarginUsage: DecimalString
  maxDrawdown: DecimalString
  stopAfterConsecutiveLosses: number
}

export interface RiskAlert {
  id: string
  severity: RiskSeverity
  title: string
  message: string
  createdAt: string
}

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

export interface PnlBreakdown {
  label: string
  value: DecimalString
  color: string
}

export interface PnlReport {
  cumulative: DecimalString
  today: DecimalString
  optionPremium: DecimalString
  realizedSpot: DecimalString
  floatingSpot: DecimalString
  fees: DecimalString
  points: ChartPoint[]
  breakdown: PnlBreakdown[]
}

export interface AuditEvent {
  id: string
  kind: string
  message: string
  createdAt: string
}

export interface StrategyPreview {
  candidate: StrategyCandidate
  maxProfit: DecimalString
  reservedAsset: DecimalString
  protectedLimitPrice: DecimalString
  summary: string[]
}

export interface AppSnapshot {
  settings: AppSettings
  metrics: MetricCard[]
  balances: BalanceRow[]
  candidates: StrategyCandidate[]
  orders: StrategyOrder[]
  pnl: PnlReport
  risk: RiskSnapshot
  autoRule: AutoInvestmentRule
  auditRecent: AuditEvent[]
}

export interface ConnectBinanceRequest {
  apiKey: string
  apiSecret: string
  live: boolean
}

export interface StrategyRequest {
  candidateId?: string
  symbol: string
  optionSymbol?: string
  direction: StrategyDirection
  expiry: string
  strike: DecimalString
  quantity: DecimalString
  live: boolean
  confirmLive: boolean
}

export interface ToggleAutoRequest {
  enabled: boolean
  confirmLive: boolean
}
