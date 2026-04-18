import { invoke } from '@tauri-apps/api/core'
import { mockCandidates, mockSnapshot } from '@/services/mock'
import type {
  AppSettings,
  AppSnapshot,
  AutoInvestmentRule,
  BalanceRow,
  ConnectBinanceRequest,
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

async function call<T>(command: string, args?: Record<string, unknown>, fallback?: () => T): Promise<T> {
  if (isTauriRuntime()) {
    return invoke<T>(command, args)
  }
  if (!fallback) {
    throw new Error(`命令 ${command} 只能在 Tauri 运行时执行`)
  }
  await new Promise(resolve => setTimeout(resolve, 120))
  return fallback()
}

export const api = {
  getAppSnapshot: () => call<AppSnapshot>('get_app_snapshot', undefined, () => structuredClone(mockSnapshot)),
  connectBinance: (request: ConnectBinanceRequest) =>
    call<AppSettings>('connect_binance', { request }, () => ({
      ...mockSnapshot.settings,
      connected: true,
      mode: request.live ? 'live' : 'simulation',
      apiKeyMasked: `${request.apiKey.slice(0, 4)}...${request.apiKey.slice(-4)}`,
    })),
  scanCandidates: () => call<StrategyCandidate[]>('scan_candidates', undefined, () => structuredClone(mockCandidates)),
  previewStrategy: (request: StrategyRequest) =>
    call<StrategyPreview>('preview_strategy', { request }, () => ({
      candidate: {
        id: request.candidateId ?? 'manual-preview',
        symbol: request.symbol,
        optionSymbol: request.optionSymbol ?? `BTC-${request.expiry.replaceAll('-', '').slice(2)}-${request.strike}-${request.direction === 'sellPut' ? 'P' : 'C'}`,
        direction: request.direction,
        expiry: request.expiry,
        strike: request.strike,
        quantity: request.quantity,
        premium: request.direction === 'sellPut' ? '186.72' : '158.36',
        annualized: request.direction === 'sellPut' ? '18.56' : '15.02',
        deltaAbs: '0.24',
        ivRank: '51',
        exerciseProbability: '22.50',
        breakeven: request.direction === 'sellPut' ? '71813.28' : '76158.36',
        recommendation: 'watch',
        reason: '浏览器 mock 预览',
      },
      maxProfit: request.direction === 'sellPut' ? '186.72' : '158.36',
      reservedAsset: request.direction === 'sellPut' ? '720' : request.quantity,
      protectedLimitPrice: request.direction === 'sellPut' ? '183.92' : '155.98',
      summary: ['快速成交：以受保护限价尝试即时成交', '到期 ITM 后自动现货补齐', '所有动作写入本地审计日志'],
    })),
  placeManualStrategy: (request: StrategyRequest) =>
    call<StrategyOrder>('place_manual_strategy', { request }, () => ({
      id: `MO-WEB-${Date.now()}`,
      symbol: request.symbol,
      optionSymbol: request.optionSymbol ?? `BTC-${request.expiry.replaceAll('-', '').slice(2)}-${request.strike}-${request.direction === 'sellPut' ? 'P' : 'C'}`,
      direction: request.direction,
      status: 'running',
      expiry: request.expiry,
      strike: request.strike,
      quantity: request.quantity,
      premium: request.direction === 'sellPut' ? '186.72' : '158.36',
      estimatedPnl: request.direction === 'sellPut' ? '186.72' : '158.36',
      breakeven: request.direction === 'sellPut' ? '71813.28' : '76158.36',
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      settlementAction: request.direction === 'sellPut'
        ? '若到期低于执行价，自动买入 BTC 补齐换币'
        : '若到期高于执行价，自动卖出 BTC 补齐换币',
    })),
  saveAutoRule: (rule: AutoInvestmentRule) => call<AutoInvestmentRule>('save_auto_rule', { rule }, () => structuredClone(rule)),
  toggleAutoStrategy: (request: ToggleAutoRequest) =>
    call<AppSettings>('toggle_auto_strategy', { request }, () => ({
      ...mockSnapshot.settings,
      autoEnabled: request.enabled,
    })),
  listOrders: () => call<StrategyOrder[]>('list_orders', undefined, () => structuredClone(mockSnapshot.orders)),
  getPositions: () => call<BalanceRow[]>('get_positions', undefined, () => structuredClone(mockSnapshot.balances)),
  getPnlReport: () => call<PnlReport>('get_pnl_report', undefined, () => structuredClone(mockSnapshot.pnl)),
  getRiskSnapshot: () => call<RiskSnapshot>('get_risk_snapshot', undefined, () => structuredClone(mockSnapshot.risk)),
  exportAuditLogs: () => call<string>('export_audit_logs', undefined, () => JSON.stringify(mockSnapshot.auditRecent, null, 2)),
}
