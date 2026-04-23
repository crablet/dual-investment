// Tauri IPC API 封装。
//
// 前端所有后端调用都集中在这里。普通浏览器环境没有 Tauri runtime 时直接抛出明确错误，
// 不提供 mock 数据或浏览器 fallback，避免把实盘路径和演示路径混在一起。

import { invoke } from '@tauri-apps/api/core'
import type {
  AppSettings,
  AppSnapshot,
  AutoInvestmentRule,
  CandidateScanResult,
  ConnectBinanceRequest,
  StrategyOrder,
  StrategyRequest,
  ToggleAutoRequest,
} from '@/types/domain'

const isTauriRuntime = () =>
  typeof window !== 'undefined' && Reflect.has(window, '__TAURI_INTERNALS__')

/**
 * 调用 Tauri command。
 *
 * command 名称保持 Rust snake_case，方法名在 `api` 对象中转换成前端 camelCase。
 */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauriRuntime()) {
    return invoke<T>(command, args)
  }
  throw new Error(`命令 ${command} 只能在 Tauri 客户端中执行`)
}

export const api = {
  getAppSnapshot: () => call<AppSnapshot>('get_app_snapshot'),
  connectBinance: (request: ConnectBinanceRequest) =>
    call<AppSettings>('connect_binance', { request }),
  connectSavedBinance: () => call<AppSettings>('connect_saved_binance'),
  forgetBinanceCredentials: () => call<AppSettings>('forget_binance_credentials'),
  scanCandidates: () => call<CandidateScanResult>('scan_candidates'),
  placeManualStrategy: (request: StrategyRequest) =>
    call<StrategyOrder>('place_manual_strategy', { request }),
  saveAutoRule: (rule: AutoInvestmentRule) => call<AutoInvestmentRule>('save_auto_rule', { rule }),
  toggleAutoStrategy: (request: ToggleAutoRequest) =>
    call<AppSettings>('toggle_auto_strategy', { request }),
  recoverOrderAttempt: (attemptId: string) =>
    call<StrategyOrder>('recover_order_attempt', { attemptId }),
  syncLiveState: () => call<AppSnapshot>('sync_live_state'),
  runSettlementScan: () => call<StrategyOrder[]>('run_settlement_scan'),
  runAutoOnce: () => call<StrategyOrder | null>('run_auto_once'),
  exportAuditLogs: () => call<string>('export_audit_logs'),
}
