import { invoke } from '@tauri-apps/api/core'
import type {
  AppSettings,
  AppSnapshot,
  AutoInvestmentRule,
  BalanceRow,
  ConnectBinanceRequest,
  OrderAttemptView,
  PnlReport,
  RiskSnapshot,
  StrategyCandidate,
  StrategyOrder,
  StrategyPreview,
  StrategyRequest,
  ToggleAutoRequest,
} from '@/types/domain'

const isTauriRuntime = () =>
  typeof window !== 'undefined' && Reflect.has(window, '__TAURI_INTERNALS__')

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauriRuntime()) {
    return invoke<T>(command, args)
  }
  throw new Error(`命令 ${command} 只能在 Tauri 实盘客户端中执行`)
}

export const api = {
  getAppSnapshot: () => call<AppSnapshot>('get_app_snapshot'),
  connectBinance: (request: ConnectBinanceRequest) =>
    call<AppSettings>('connect_binance', { request }),
  connectSavedBinance: () => call<AppSettings>('connect_saved_binance'),
  forgetBinanceCredentials: () => call<AppSettings>('forget_binance_credentials'),
  scanCandidates: () => call<StrategyCandidate[]>('scan_candidates'),
  previewStrategy: (request: StrategyRequest) =>
    call<StrategyPreview>('preview_strategy', { request }),
  placeManualStrategy: (request: StrategyRequest) =>
    call<StrategyOrder>('place_manual_strategy', { request }),
  saveAutoRule: (rule: AutoInvestmentRule) => call<AutoInvestmentRule>('save_auto_rule', { rule }),
  toggleAutoStrategy: (request: ToggleAutoRequest) =>
    call<AppSettings>('toggle_auto_strategy', { request }),
  listOrders: () => call<StrategyOrder[]>('list_orders'),
  listOrderAttempts: () => call<OrderAttemptView[]>('list_order_attempts'),
  syncLiveState: () => call<AppSnapshot>('sync_live_state'),
  runSettlementScan: () => call<StrategyOrder[]>('run_settlement_scan'),
  runAutoOnce: () => call<StrategyOrder | null>('run_auto_once'),
  getPositions: () => call<BalanceRow[]>('get_positions'),
  getPnlReport: () => call<PnlReport>('get_pnl_report'),
  getRiskSnapshot: () => call<RiskSnapshot>('get_risk_snapshot'),
  exportAuditLogs: () => call<string>('export_audit_logs'),
}
