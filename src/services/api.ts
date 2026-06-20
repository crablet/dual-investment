// 封装 Tauri 调用。没有桌面运行环境时返回错误，不提供模拟业务数据。

import type { LocalizedMessage } from '@/i18n'
import type {
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

/** 先校验错误消息结构，再读取自动重连标志。 */
function isLocalizedMessage(value: unknown, depth = 0): value is LocalizedMessage {
  if (depth > 20 || typeof value !== 'object' || value === null
    || !('code' in value) || typeof value.code !== 'string'
    || !('params' in value) || typeof value.params !== 'object' || value.params === null || Array.isArray(value.params)) {
    return false
  }
  return Object.values(value.params).every(param => typeof param === 'string' || isLocalizedMessage(param, depth + 1))
}

/** 通信错误显示给用户，但不触发自动重连。 */
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
    call<void>('recover_order_attempt', { attemptId }),
  recoverFundingAttempt: (attemptId: string) =>
    call<FundingAttemptView>('recover_funding_attempt', { attemptId }),
  resolveFundingAttempt: (request: ResolveFundingAttemptRequest) =>
    call<FundingAttemptView>('resolve_funding_attempt', { request }),
  finishSettlementSweep: (request: FinishSettlementSweepRequest) =>
    call<SettlementRecoveryView>('finish_settlement_sweep', { request }),
  syncLiveState: () => call<void>('sync_live_state'),
  getLogSnapshot: () => call<LogSnapshot>('get_log_snapshot'),
  openCurrentLogFile: () => call<void>('open_current_log_file'),
  openLogFolder: () => call<void>('open_log_folder'),
  clearLogFiles: () => call<LogSnapshot>('clear_log_files'),
  listenOrderProgress: (handler: (event: OrderProgressEvent) => void) => {
    if (!isTauriRuntime()) {
      // 浏览器没有 Tauri 事件源，返回空清理函数。
      return Promise.resolve(() => undefined)
    }
    return listen<OrderProgressEvent>('order-progress', event => handler(event.payload))
  },
}
