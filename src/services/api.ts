// Tauri IPC API 封装。
//
// 前端所有后端调用都集中在这里。普通浏览器环境没有 Tauri runtime 时直接抛出明确错误，
// 不提供 mock 数据或浏览器 fallback，避免把实盘路径和演示路径混在一起。

import type { LocalizedMessage } from '@/i18n'
import type {
  AdoptableOptionPositionView,
  AdoptOptionPositionsRequest,
  AppSettings,
  AppSnapshot,
  CommandErrorPayload,
  ConnectBinanceRequest,
  FinishSettlementSweepRequest,
  FundingAttemptView,
  LogSnapshot,
  OrderProgressEvent,
  ResolveFundingAttemptRequest,
  SettlementRecoveryView,
  StrategyCandidate,
  StrategyOrder,
  StrategyPreview,
  StrategyPreviewRequest,
  StrategyRequest,
} from '@/types/domain'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { message as localized } from '@/i18n'

class ClientCommandError extends Error {
  constructor(readonly payload: CommandErrorPayload) {
    super(payload.message.code)
    this.name = 'ClientCommandError'
  }
}

function isTauriRuntime() {
  return typeof window !== 'undefined' && Reflect.has(window, '__TAURI_INTERNALS__')
}

/** 校验嵌套的消息参数，避免损坏的 IPC 数据获得业务重试资格。 */
function isLocalizedMessage(value: unknown, depth = 0): value is LocalizedMessage {
  if (depth > 20 || typeof value !== 'object' || value === null
    || !('code' in value) || typeof value.code !== 'string'
    || !('params' in value) || typeof value.params !== 'object' || value.params === null || Array.isArray(value.params)) {
    return false
  }
  return Object.values(value.params).every(param => typeof param === 'string' || isLocalizedMessage(param, depth + 1))
}

/** Tauri 自身的传输错误没有业务重试许可，仍须向用户显示错误。 */
export function commandError(caught: unknown): CommandErrorPayload {
  if (caught instanceof ClientCommandError)
    return caught.payload
  if (typeof caught === 'object' && caught !== null
    && 'message' in caught && isLocalizedMessage(caught.message)
    && 'canAutoReconnect' in caught && typeof caught.canAutoReconnect === 'boolean') {
    return { message: caught.message, canAutoReconnect: caught.canAutoReconnect }
  }
  const detail = caught instanceof Error ? caught.message : typeof caught === 'string' ? caught : undefined
  return {
    message: detail ? localized('ui.transportError', { detail }) : localized('ui.clientRequestFailedCheckTheErrorRecords'),
    canAutoReconnect: false,
  }
}

/**
 * 调用 Tauri command。
 *
 * command 名称保持 Rust snake_case，方法名在 `api` 对象中转换成前端 camelCase。
 */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauriRuntime()) {
    return invoke<T>(command, args)
  }
  throw new ClientCommandError({ message: localized('ui.commandRequiresTheTauriDesktopClient', { p0: command }), canAutoReconnect: false })
}

export const api = {
  getAppSnapshot: () => call<AppSnapshot>('get_app_snapshot'),
  connectBinance: (request: ConnectBinanceRequest) =>
    call<AppSettings>('connect_binance', { request }),
  connectSavedBinance: () => call<AppSettings>('connect_saved_binance'),
  forgetBinanceCredentials: () => call<AppSettings>('forget_binance_credentials'),
  scanCandidates: () => call<StrategyCandidate[]>('scan_candidates'),
  previewStrategy: (request: StrategyPreviewRequest) =>
    call<StrategyPreview>('preview_strategy', { request }),
  placeManualStrategy: (request: StrategyRequest) =>
    call<StrategyOrder>('place_manual_strategy', { request }),
  recoverOrderAttempt: (attemptId: string) =>
    call<StrategyOrder>('recover_order_attempt', { attemptId }),
  recoverFundingAttempt: (attemptId: string) =>
    call<FundingAttemptView>('recover_funding_attempt', { attemptId }),
  resolveFundingAttempt: (request: ResolveFundingAttemptRequest) =>
    call<FundingAttemptView>('resolve_funding_attempt', { request }),
  finishSettlementSweep: (request: FinishSettlementSweepRequest) =>
    call<SettlementRecoveryView>('finish_settlement_sweep', { request }),
  listAdoptableOptionPositions: () =>
    call<AdoptableOptionPositionView[]>('list_adoptable_option_positions'),
  adoptOptionPositions: (request: AdoptOptionPositionsRequest) =>
    call<StrategyOrder[]>('adopt_option_positions', { request }),
  syncLiveState: () => call<void>('sync_live_state'),
  runSettlementScan: () => call<StrategyOrder[]>('run_settlement_scan'),
  getLogSnapshot: () => call<LogSnapshot>('get_log_snapshot'),
  openCurrentLogFile: () => call<void>('open_current_log_file'),
  openLogFolder: () => call<void>('open_log_folder'),
  clearLogFiles: () => call<LogSnapshot>('clear_log_files'),
  listenOrderProgress: (handler: (event: OrderProgressEvent) => void) => {
    if (!isTauriRuntime()) {
      // 普通浏览器只用于布局检查；没有事件源时返回空清理函数，而不是模拟进度事件。
      return Promise.resolve(() => undefined)
    }
    return listen<OrderProgressEvent>('order-progress', event => handler(event.payload))
  },
}
