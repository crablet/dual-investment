// 应用级 Pinia store。
//
// 这里是前端唯一的跨页面状态源：保存 Tauri 快照、请求 loading/error、当前视图，以及
// 从快照派生出的总览指标。真实业务计算仍以 Rust 返回的快照为准，前端只做展示层派生。

import type { LocalizedMessage } from '@/i18n'
import type {
  AppSettings,
  AppSnapshot,
  ConnectBinanceRequest,
  FundingAttemptManualResolution,
  OrderProgressEvent,
  OrderProgressStatus,
  StrategyOrder,
  StrategyRequest,
} from '@/types/domain'
import type {
  MetricCardModel,
  OrderProgressNotice,
  OrderProgressView,
} from '@/types/view'
import { defineStore } from 'pinia'
import { computed, shallowRef } from 'vue'
import { message as localized, messageText, t } from '@/i18n'
import { api, commandError } from '@/services/api'
import { buildMetrics } from '@/stores/appMetrics'
import {
  formatReconnectDelay,
  reconnectBackoffDelays,
  reconnectDelayAfterAttempt,
} from '@/stores/autoReconnect'
import {
  createInitialOrderProgress,
  createOrderProgressSteps,
  makeOrderFlowId,
  markCurrentOrderStepError,
  orderProgressSuccessMessages,
  progressStatusFromSteps,
} from '@/stores/orderProgress'
import { isPositiveDecimalString } from '@/utils/decimal'

type RequestKey
  = | 'snapshot'
    | 'connect'
    | 'credentials'
    | 'scan'
    | 'sync'
    | 'recover'
    | 'order'

/** 请求包装选项。silent 请求用于后台轮询，不清空前台错误提示。 */
interface RunOptions {
  silent?: boolean
}

interface ConnectSavedOptions extends RunOptions {
  autoReconnect?: boolean
}

type AutoReconnectState = 'idle' | 'waiting' | 'connecting' | 'paused'
type PlaceStrategyRequest = Omit<StrategyRequest, 'clientFlowId'>

/** 应用内视图 ID；导航切换统一走 `openView`，以触发对应页面刷新。 */
export type AppViewId
  = | 'overview'
    | 'dual'
    | 'orders'
    | 'portfolio'
    | 'analytics'
    | 'settings'

function emptyPending(): Record<RequestKey, number> {
  return {
    snapshot: 0,
    connect: 0,
    credentials: 0,
    scan: 0,
    sync: 0,
    recover: 0,
    order: 0,
  }
}

export const useAppStore = defineStore('app', () => {
  // 快照使用 shallowRef：后端每次返回完整对象，前端通过替换根引用触发更新。
  const snapshot = shallowRef<AppSnapshot>()
  const pending = shallowRef(emptyPending())
  const foregroundPending = shallowRef(0)
  const error = shallowRef<LocalizedMessage>()
  const backgroundError = shallowRef<LocalizedMessage>()
  const activeView = shallowRef<AppViewId>('overview')
  const orderProgress = shallowRef<OrderProgressView>()
  const orderNotice = shallowRef<OrderProgressNotice>()
  const unlistenOrderProgress = shallowRef<(() => void)>()
  const autoReconnectState = shallowRef<AutoReconnectState>('idle')
  const autoReconnectAttempts = shallowRef(0)
  const autoReconnectDelayMs = shallowRef(reconnectBackoffDelays[0])
  const lastAutoReconnectError = shallowRef<LocalizedMessage>()
  let snapshotRequestId = 0

  const loading = computed(() => foregroundPending.value > 0)
  const settings = computed(() => snapshot.value?.settings)
  const account = computed(() => snapshot.value?.account)
  const candidates = computed(() => snapshot.value?.candidates ?? [])
  const orders = computed(() => snapshot.value?.orders ?? [])
  const positions = computed(() => snapshot.value?.positions ?? [])
  const deliveries = computed(() => snapshot.value?.deliveries ?? [])
  const positionSync = computed(() => snapshot.value?.positionSync)
  const pnl = computed(() => snapshot.value?.pnl)
  const portfolio = computed(() => snapshot.value?.portfolio)
  const orderAttempts = computed(() => snapshot.value?.orderAttempts ?? [])
  const fundingAttempts = computed(() => snapshot.value?.fundingAttempts ?? [])
  const settlementRecoveries = computed(() => snapshot.value?.settlementRecoveries ?? [])
  const openPositions = computed(() =>
    positions.value.filter(position =>
      position.status === 'open' && isPositiveDecimalString(position.quantity),
    ),
  )
  // 指标卡从快照派生，避免再通过 IPC 请求 Rust 做轻量 UI 聚合。
  const metrics = computed<MetricCardModel[]>(() => buildMetrics(
    settings.value,
    account.value,
    pnl.value,
  ))
  const canAutoReconnect = computed(() =>
    Boolean(
      settings.value?.savedCredentials
      && !settings.value.connected
      && autoReconnectState.value !== 'paused',
    ),
  )
  const autoReconnectPaused = computed(() => autoReconnectState.value === 'paused')
  const autoReconnectMessage = computed(() => {
    if (settings.value?.connected || !settings.value?.savedCredentials) {
      return undefined
    }
    if (autoReconnectState.value === 'connecting') {
      return t('ui.automaticallyConnectingToBinanceWithSavedCredentials')
    }
    if (autoReconnectState.value === 'paused') {
      return t('ui.automaticReconnectionPaused', { p0: messageText(lastAutoReconnectError.value ?? localized('ui.checkSavedCredentialsAndBinanceApiPermissions')) })
    }
    if (lastAutoReconnectError.value) {
      return t('ui.automaticReconnectionWillRetryInAboutLast', { p0: formatReconnectDelay(autoReconnectDelayMs.value), p1: messageText(lastAutoReconnectError.value) })
    }
    return t('ui.savedCredentialsFoundBinanceConnectionWillBe')
  })

  async function run<T>(
    key: RequestKey,
    task: () => Promise<T>,
    options: RunOptions = {},
  ): Promise<T | undefined> {
    // 前台错误和后台错误分开保存：静默刷新/重连失败不应覆盖用户正在处理的表单或交易错误。
    const silent = options.silent ?? false
    setPending(key, 1)
    if (!silent) {
      foregroundPending.value += 1
      error.value = undefined
    }
    try {
      const result = await task()
      if (silent) {
        backgroundError.value = undefined
      }
      return result
    }
    catch (caught) {
      const message = commandError(caught).message
      if (silent) {
        backgroundError.value = message
      }
      else {
        error.value = message
      }
      return undefined
    }
    finally {
      setPending(key, -1)
      if (!silent) {
        foregroundPending.value = Math.max(0, foregroundPending.value - 1)
      }
    }
  }

  /** 增减单个操作维度的 pending 计数，支持同类请求并发时保持 loading 准确。 */
  function setPending(key: RequestKey, delta: number) {
    pending.value = {
      ...pending.value,
      [key]: Math.max(0, pending.value[key] + delta),
    }
  }

  function isLoading(key: RequestKey) {
    return pending.value[key] > 0
  }

  async function bindOrderProgressEvents() {
    if (unlistenOrderProgress.value) {
      return
    }
    try {
      // 普通浏览器环境下 `api.listenOrderProgress` 会返回空取消函数，不模拟真实下单进度。
      unlistenOrderProgress.value = await api.listenOrderProgress(applyOrderProgressEvent)
    }
    catch (caught) {
      backgroundError.value = commandError(caught).message
    }
  }

  function disposeOrderProgressEvents() {
    unlistenOrderProgress.value?.()
    unlistenOrderProgress.value = undefined
  }

  function clearOrderProgress() {
    orderProgress.value = undefined
  }

  function startOrderProgress(flowId: string, optionSymbol: string): OrderProgressView {
    const progress = createInitialOrderProgress(flowId, optionSymbol)
    orderProgress.value = progress
    return progress
  }

  function applyOrderProgressEvent(event: OrderProgressEvent) {
    // Tauri 事件可能先于 `placeStrategy` 的本地进度对象到达，先建临时视图再等最终快照校正。
    const current = orderProgress.value?.flowId === event.flowId
      ? orderProgress.value
      : {
          flowId: event.flowId,
          status: 'running' as OrderProgressStatus,
          steps: createOrderProgressSteps(),
        }

    const steps = current.steps.map(step =>
      step.id === event.stage
        ? {
            ...step,
            status: event.status,
            message: event.message,
          }
        : step,
    )
    orderProgress.value = {
      ...current,
      status: progressStatusFromSteps(steps),
      steps,
      optionSymbol: event.optionSymbol ?? current.optionSymbol,
      exchangeOrderId: event.exchangeOrderId ?? current.exchangeOrderId,
      error: event.status === 'error' ? event.message : current.error,
    }
  }

  function finishOrderProgressSuccess(flowId: string, order: StrategyOrder) {
    // 进度面板只做即时反馈；订单事实仍以随后刷新到的 Rust 快照和本地恢复记录为准。
    const current = orderProgress.value?.flowId === flowId
      ? orderProgress.value
      : startOrderProgress(flowId, order.optionSymbol)
    const steps = current.steps.map((step) => {
      const status: OrderProgressStatus = step.status === 'error' ? 'error' : 'success'
      return {
        ...step,
        status,
        message: step.id === 'persist'
          ? localized('ui.orderSavedExchangeStatus', { p0: order.exchangeStatus })
          : step.message ?? orderProgressSuccessMessages[step.id],
      }
    })
    orderProgress.value = {
      ...current,
      status: 'success',
      steps,
      optionSymbol: order.optionSymbol,
      exchangeOrderId: order.exchangeOrderId ?? current.exchangeOrderId,
      order,
      error: undefined,
    }
    orderNotice.value = {
      type: 'success',
      message: localized('ui.orderCreatedExchangeStatus', { p0: order.exchangeStatus }),
    }
  }

  function finishOrderProgressError(flowId: string, message: LocalizedMessage) {
    const current = orderProgress.value?.flowId === flowId
      ? orderProgress.value
      : {
          flowId,
          status: 'running' as OrderProgressStatus,
          steps: createOrderProgressSteps('validate', 'running', localized('ui.submittingOrderRequest')),
        }
    const hasErrorStep = current.steps.some(step => step.status === 'error')
    const steps = hasErrorStep
      ? current.steps
      : markCurrentOrderStepError(current.steps, message)
    orderProgress.value = {
      ...current,
      status: 'error',
      steps,
      error: message,
    }
    orderNotice.value = {
      type: 'error',
      message,
    }
  }

  async function refresh(options: RunOptions = {}) {
    // 多个动作可同时要求读取本地状态；较早发出的快照不能覆盖动作后的读取结果。
    const requestId = ++snapshotRequestId
    const next = await run('snapshot', () => api.getAppSnapshot(), options)
    if (next && requestId === snapshotRequestId) {
      snapshot.value = next
      reconcileAutoReconnectSettings(next.settings)
    }
  }

  /** 应用启动：先读取快照，再用 keyring 凭据连接；既有订单由 Rust 后台按到期时间自动处理。 */
  async function bootstrap() {
    await refresh()
    if (snapshot.value?.settings.savedCredentials && !snapshot.value.settings.connected) {
      await tryAutoReconnectSaved()
    }
  }

  async function connect(request: ConnectBinanceRequest) {
    resetAutoReconnect()
    const next = await run('connect', () => api.connectBinance(request))
    patchSettings(next)
    if (next?.connected) {
      resetAutoReconnect()
      await syncLiveState()
    }
    else {
      await refresh({ silent: true })
    }
  }

  async function connectSaved(options: ConnectSavedOptions = {}) {
    if (!options.autoReconnect) {
      resetAutoReconnect()
    }
    const next = await run('connect', async () => {
      try {
        return await api.connectSavedBinance()
      }
      catch (caught) {
        const failure = commandError(caught)
        if (options.autoReconnect) {
          lastAutoReconnectError.value = failure.message
          autoReconnectState.value = failure.canAutoReconnect ? 'waiting' : 'paused'
        }
        throw failure
      }
    }, options)
    patchSettings(next)
    if (next?.connected) {
      // 重连入口只同步事实；后台继续处理此前已确认的既有订单。
      resetAutoReconnect()
      await syncLiveState(options)
      return true
    }
    if (options.autoReconnect && autoReconnectState.value === 'connecting') {
      lastAutoReconnectError.value = localized('ui.connectionDidNotReturnASuccessfulStatus')
      autoReconnectState.value = 'paused'
    }
    await refresh({ silent: true })
    return false
  }

  async function forgetCredentials() {
    const next = await run('credentials', () => api.forgetBinanceCredentials())
    patchSettings(next)
    if (next) {
      resetAutoReconnect()
      await refresh()
    }
  }

  async function tryAutoReconnectSaved() {
    if (!canAutoReconnect.value || isLoading('connect')) {
      return false
    }

    autoReconnectState.value = 'connecting'
    autoReconnectAttempts.value += 1
    const connected = await connectSaved({ silent: true, autoReconnect: true })
    if (connected) {
      return true
    }

    if (autoReconnectPaused.value) {
      autoReconnectDelayMs.value = reconnectBackoffDelays.at(-1) ?? reconnectBackoffDelays[0]
      return false
    }

    autoReconnectState.value = 'waiting'
    autoReconnectDelayMs.value = reconnectDelayAfterAttempt(autoReconnectAttempts.value)
    return false
  }

  async function scan(options: RunOptions = {}) {
    await run('scan', () => api.scanCandidates(), options)
    await refresh({ silent: true })
  }

  async function openView(view: AppViewId) {
    activeView.value = view
    await refreshViewData(view)
  }

  /** 导航读取本地状态；已连接且双币赢没有缓存候选时会扫描。 */
  async function refreshViewData(view: AppViewId = activeView.value, options: RunOptions = {}) {
    if (view === 'dual' && settings.value?.connected && candidates.value.length === 0) {
      if (!isLoading('scan')) {
        await scan(options)
      }
      return
    }

    if (!isLoading('snapshot')) {
      await refresh(options)
    }
  }

  async function syncLiveState(options: RunOptions = {}) {
    await run('sync', () => api.syncLiveState(), options)
    // 同步可能部分完成后失败；本地状态仍须展示，且不能覆盖上面的业务错误。
    await refresh({ silent: true })
  }

  async function recoverAttempt(attemptId: string) {
    await run('recover', () => api.recoverOrderAttempt(attemptId))
    await refresh({ silent: true })
  }

  async function recoverFundingAttempt(attemptId: string) {
    await run('recover', () => api.recoverFundingAttempt(attemptId))
    await refresh({ silent: true })
  }

  async function resolveFundingAttempt(attemptId: string, resolution: FundingAttemptManualResolution) {
    await run('recover', () =>
      api.resolveFundingAttempt({
        attemptId,
        resolution,
        confirmManual: true,
      }))
    await refresh({ silent: true })
  }

  async function finishSettlementSweep(batchId: string) {
    await run('recover', () => api.finishSettlementSweep({
      batchId,
      confirmManual: true,
    }))
    await refresh({ silent: true })
  }

  async function placeStrategy(request: PlaceStrategyRequest) {
    const flowId = makeOrderFlowId()
    const orderRequest: StrategyRequest = { ...request, clientFlowId: flowId }
    startOrderProgress(flowId, orderRequest.optionSymbol)
    const order = await run('order', () => api.placeManualStrategy(orderRequest))
    if (order) {
      finishOrderProgressSuccess(flowId, order)
      await refresh({ silent: true })
      return
    }
    finishOrderProgressError(flowId, error.value ?? localized('ui.orderFailedCheckTheErrorAndRecovery2'))
    // 下单失败后的静默刷新用于暴露后端保存的恢复记录；不在前端重试真实订单。
    await refresh({ silent: true })
  }

  function patchSettings(next?: AppSettings) {
    if (!next) {
      return
    }
    snapshotRequestId += 1
    if (snapshot.value) {
      snapshot.value = { ...snapshot.value, settings: next }
    }
    reconcileAutoReconnectSettings(next)
  }

  function reconcileAutoReconnectSettings(next: AppSettings) {
    if (next.connected || !next.savedCredentials) {
      resetAutoReconnect()
    }
  }

  function resetAutoReconnect() {
    autoReconnectState.value = 'idle'
    autoReconnectAttempts.value = 0
    autoReconnectDelayMs.value = reconnectBackoffDelays[0]
    lastAutoReconnectError.value = undefined
  }

  return {
    snapshot,
    loading,
    error,
    backgroundError,
    autoReconnectDelayMs,
    autoReconnectPaused,
    autoReconnectMessage,
    canAutoReconnect,
    activeView,
    orderProgress,
    orderNotice,
    settings,
    metrics,
    account,
    candidates,
    orders,
    positions,
    deliveries,
    positionSync,
    pnl,
    portfolio,
    orderAttempts,
    fundingAttempts,
    settlementRecoveries,
    openPositions,
    isLoading,
    bindOrderProgressEvents,
    disposeOrderProgressEvents,
    clearOrderProgress,
    bootstrap,
    refresh,
    connect,
    connectSaved,
    tryAutoReconnectSaved,
    forgetCredentials,
    scan,
    syncLiveState,
    recoverAttempt,
    recoverFundingAttempt,
    resolveFundingAttempt,
    finishSettlementSweep,
    placeStrategy,
    openView,
    refreshViewData,
  }
})
