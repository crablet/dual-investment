// 应用级 Pinia store。
//
// 这里是前端唯一的跨页面状态源：保存 Tauri 快照、请求 loading/error、当前视图，以及
// 从快照派生出的指标和收益构成。真实业务计算仍以 Rust 返回的快照为准，前端只做展示层派生。

import type {
  AdoptableOptionPositionView,
  AdoptOptionPositionsRequest,
  AppSettings,
  AppSnapshot,
  AutoInvestmentRule,
  ConnectBinanceRequest,
  FundingAttemptManualResolution,
  OrderProgressEvent,
  OrderProgressStatus,
  StrategyOrder,
  StrategyRequest,
  ToggleAutoRequest,
} from '@/types/domain'
import type {
  MetricCardModel,
  OrderProgressNotice,
  OrderProgressView,
  PnlBreakdownItem,
} from '@/types/view'
import { defineStore } from 'pinia'
import { computed, shallowRef } from 'vue'
import { api } from '@/services/api'
import { buildMetrics, buildPnlBreakdown } from '@/stores/appMetrics'
import {
  formatReconnectDelay,
  isPermanentSavedConnectionError,
  reconnectBackoffDelays,
  reconnectDelayAfterAttempt,
} from '@/stores/autoReconnect'
import {
  createInitialOrderProgress,
  createOrderProgressFromOrder,
  createOrderProgressSteps,
  makeOrderFlowId,
  markCurrentOrderStepError,
  orderProgressSuccessMessages,
  progressStatusFromSteps,
} from '@/stores/orderProgress'

type RequestKey
  = | 'snapshot'
    | 'connect'
    | 'credentials'
    | 'scan'
    | 'sync'
    | 'settlement'
    | 'auto'
    | 'recover'
    | 'adopt'
    | 'order'
    | 'rule'

/** 请求包装选项。silent 请求用于后台轮询，不清空前台错误提示。 */
interface RunOptions {
  silent?: boolean
}

interface ConnectSavedOptions extends RunOptions {
  autoReconnect?: boolean
}

type AutoReconnectState = 'idle' | 'waiting' | 'connecting' | 'paused'

/** 应用内视图 ID；导航切换统一走 `openView`，以触发对应页面刷新。 */
export type AppViewId
  = | 'overview'
    | 'dual'
    | 'orders'
    | 'portfolio'
    | 'analytics'
    | 'risk'
    | 'settings'

function emptyPending(): Record<RequestKey, number> {
  return {
    snapshot: 0,
    connect: 0,
    credentials: 0,
    scan: 0,
    sync: 0,
    settlement: 0,
    auto: 0,
    recover: 0,
    adopt: 0,
    order: 0,
    rule: 0,
  }
}

export const useAppStore = defineStore('app', () => {
  // 快照使用 shallowRef：后端每次返回完整对象，前端通过替换根引用触发更新。
  const snapshot = shallowRef<AppSnapshot>()
  const pending = shallowRef(emptyPending())
  const foregroundPending = shallowRef(0)
  const backgroundPending = shallowRef(0)
  const error = shallowRef<string>()
  const backgroundError = shallowRef<string>()
  const activeView = shallowRef<AppViewId>('overview')
  const orderProgress = shallowRef<OrderProgressView>()
  const orderNotice = shallowRef<OrderProgressNotice>()
  const adoptableOptionPositions = shallowRef<AdoptableOptionPositionView[]>([])
  const unlistenOrderProgress = shallowRef<(() => void)>()
  const autoReconnectState = shallowRef<AutoReconnectState>('idle')
  const autoReconnectAttempts = shallowRef(0)
  const autoReconnectDelayMs = shallowRef(reconnectBackoffDelays[0])
  const lastAutoReconnectError = shallowRef<string>()

  const loading = computed(() => foregroundPending.value > 0)
  const backgroundLoading = computed(() => backgroundPending.value > 0)
  const busy = computed(() => Object.values(pending.value).some(count => count > 0))
  const settings = computed(() => snapshot.value?.settings)
  const balances = computed(() => snapshot.value?.balances ?? [])
  const candidates = computed(() => snapshot.value?.candidates ?? [])
  const scanDiagnostics = computed(() => snapshot.value?.scanDiagnostics)
  const orders = computed(() => snapshot.value?.orders ?? [])
  const pnl = computed(() => snapshot.value?.pnl)
  const risk = computed(() => snapshot.value?.risk)
  const autoRule = computed(() => snapshot.value?.autoRule)
  const health = computed(() => snapshot.value?.health)
  const orderAttempts = computed(() => snapshot.value?.orderAttempts ?? [])
  const fundingAttempts = computed(() => snapshot.value?.fundingAttempts ?? [])
  const pendingRecoveryCount = computed(() => health.value?.pendingRecoveryCount ?? 0)
  const runningOrders = computed(() =>
    orders.value.filter(order => order.status === 'running' || order.status === 'partiallyFilled'),
  )
  // 指标卡和收益构成从快照派生，避免再通过 IPC 请求 Rust 做轻量 UI 聚合。
  const metrics = computed<MetricCardModel[]>(() => buildMetrics(
    settings.value,
    balances.value,
    pnl.value,
  ))
  const pnlBreakdown = computed<PnlBreakdownItem[]>(() => buildPnlBreakdown(pnl.value))
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
      return '正在使用已保存凭据自动连接 Binance'
    }
    if (autoReconnectState.value === 'paused') {
      return `自动重连已暂停：${lastAutoReconnectError.value ?? '请检查已保存凭据和 Binance API 权限'}`
    }
    if (lastAutoReconnectError.value) {
      return `自动重连将在约 ${formatReconnectDelay(autoReconnectDelayMs.value)} 后再次尝试。上次失败：${lastAutoReconnectError.value}`
    }
    return '检测到已保存凭据，后台会自动尝试连接 Binance'
  })

  async function run<T>(
    key: RequestKey,
    task: () => Promise<T>,
    options: RunOptions = {},
  ): Promise<T | undefined> {
    // 前台错误和后台错误分开保存：静默刷新/重连失败不应覆盖用户正在处理的表单或交易错误。
    const silent = options.silent ?? false
    setPending(key, 1)
    if (silent) {
      backgroundPending.value += 1
    }
    else {
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
      const message = caught instanceof Error ? caught.message : String(caught)
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
      if (silent) {
        backgroundPending.value = Math.max(0, backgroundPending.value - 1)
      }
      else {
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

  function isBusy() {
    return busy.value
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
      backgroundError.value = caught instanceof Error ? caught.message : String(caught)
    }
  }

  function disposeOrderProgressEvents() {
    unlistenOrderProgress.value?.()
    unlistenOrderProgress.value = undefined
  }

  function clearOrderProgress() {
    orderProgress.value = undefined
  }

  function startOrderProgress(flowId: string, request: StrategyRequest): OrderProgressView {
    const progress = createInitialOrderProgress(flowId, request)
    orderProgress.value = progress
    return progress
  }

  function applyOrderProgressEvent(event: OrderProgressEvent) {
    // Tauri 事件可能先于 `placeStrategy` 的本地进度对象到达，先建临时视图再等最终快照校正。
    const current = orderProgress.value?.flowId === event.flowId
      ? orderProgress.value
      : {
          flowId: event.flowId,
          source: event.source,
          status: 'running' as OrderProgressStatus,
          startedAt: event.timestamp,
          updatedAt: event.timestamp,
          steps: createOrderProgressSteps(),
        }

    const steps = current.steps.map(step =>
      step.id === event.stage
        ? {
            ...step,
            status: event.status,
            message: event.message,
            timestamp: event.timestamp,
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
      updatedAt: event.timestamp,
    }
  }

  function finishOrderProgressSuccess(flowId: string, order: StrategyOrder) {
    // 进度面板只做即时反馈；订单事实仍以随后刷新到的 Rust 快照和本地恢复记录为准。
    const now = new Date().toISOString()
    const current = orderProgress.value?.flowId === flowId
      ? orderProgress.value
      : startOrderProgress(flowId, createOrderProgressFromOrder(flowId, order))
    const steps = current.steps.map((step) => {
      const status: OrderProgressStatus = step.status === 'error' ? 'error' : 'success'
      return {
        ...step,
        status,
        message: step.id === 'persist'
          ? `订单已保存，交易所状态 ${order.exchangeStatus}`
          : step.message ?? orderProgressSuccessMessages[step.id],
        timestamp: step.timestamp ?? now,
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
      updatedAt: now,
    }
    orderNotice.value = {
      id: `${flowId}-success-${Date.now()}`,
      type: 'success',
      message: `订单已创建，交易所状态 ${order.exchangeStatus}`,
    }
  }

  function finishOrderProgressError(flowId: string, message: string) {
    const now = new Date().toISOString()
    const current = orderProgress.value?.flowId === flowId
      ? orderProgress.value
      : {
          flowId,
          source: 'manual',
          status: 'running' as OrderProgressStatus,
          startedAt: now,
          updatedAt: now,
          steps: createOrderProgressSteps('validate', 'running', '正在提交下单请求', now),
        }
    const hasErrorStep = current.steps.some(step => step.status === 'error')
    const steps = hasErrorStep
      ? current.steps
      : markCurrentOrderStepError(current.steps, message, now)
    orderProgress.value = {
      ...current,
      status: 'error',
      steps,
      error: message,
      updatedAt: now,
    }
    orderNotice.value = {
      id: `${flowId}-error-${Date.now()}`,
      type: 'error',
      message,
    }
  }

  async function refresh(options: RunOptions = {}) {
    const next = await run('snapshot', () => api.getAppSnapshot(), options)
    if (next) {
      snapshot.value = next
      reconcileAutoReconnectSettings(next.settings)
    }
  }

  /** 应用启动：先读取快照，再尝试用 keyring 凭据自动连接，但不自动开启策略。 */
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
  }

  async function connectSaved(options: ConnectSavedOptions = {}) {
    if (!options.autoReconnect) {
      resetAutoReconnect()
    }
    const next = await run('connect', () => api.connectSavedBinance(), options)
    patchSettings(next)
    if (next?.connected) {
      // 自动重连只恢复连接和同步快照，不会开启自动策略或发起资金/订单动作。
      resetAutoReconnect()
      await syncLiveState(options)
      return true
    }
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

    const message = backgroundError.value ?? '自动连接 Binance 失败'
    lastAutoReconnectError.value = message
    if (isPermanentSavedConnectionError(message)) {
      // 凭据缺失、权限不足等人工错误需要暂停，避免后台持续打扰 Binance 接口。
      autoReconnectState.value = 'paused'
      autoReconnectDelayMs.value = reconnectBackoffDelays.at(-1) ?? reconnectBackoffDelays[0]
      return false
    }

    autoReconnectState.value = 'waiting'
    autoReconnectDelayMs.value = reconnectDelayAfterAttempt(autoReconnectAttempts.value)
    return false
  }

  async function scan(options: RunOptions = {}) {
    const next = await run('scan', () => api.scanCandidates(), options)
    if (next && snapshot.value) {
      snapshot.value = {
        ...snapshot.value,
        candidates: next.candidates,
        scanDiagnostics: next.diagnostics,
      }
    }
  }

  async function openView(view: AppViewId) {
    activeView.value = view
    await refreshViewData(view)
  }

  /** 根据视图刷新所需数据。重复点击导航只刷新数据，不会自动开启策略或下单。 */
  async function refreshViewData(view: AppViewId = activeView.value, options: RunOptions = {}) {
    if (view === 'dual') {
      if (settings.value?.connected) {
        if (!isLoading('scan')) {
          await scan(options)
        }
        return
      }
      if (!isLoading('snapshot')) {
        await refresh(options)
      }
      return
    }

    if (view === 'settings') {
      if (!isLoading('snapshot')) {
        await refresh(options)
      }
      return
    }

    if (settings.value?.connected) {
      if (!isLoading('sync')) {
        await syncLiveState(options)
      }
      return
    }

    if (!isLoading('snapshot')) {
      await refresh(options)
    }
  }

  async function syncLiveState(options: RunOptions = {}) {
    const next = await run('sync', () => api.syncLiveState(), options)
    if (next) {
      snapshot.value = next
    }
  }

  async function runSettlementScan() {
    const next = await run('settlement', () => api.runSettlementScan())
    if (next && snapshot.value) {
      snapshot.value = { ...snapshot.value, orders: mergeOrders(next, snapshot.value.orders) }
      // 结算扫描可能产生风险状态、恢复记录或资金结果，完成后拉完整快照校正局部合并。
      await refresh()
    }
  }

  async function runAutoOnce() {
    const order = await run('auto', () => api.runAutoOnce())
    if (order && snapshot.value) {
      snapshot.value = { ...snapshot.value, orders: [order, ...snapshot.value.orders] }
      await refresh()
    }
  }

  async function recoverAttempt(attemptId: string) {
    const order = await run('recover', () => api.recoverOrderAttempt(attemptId))
    if (order && snapshot.value) {
      snapshot.value = { ...snapshot.value, orders: mergeOrders([order], snapshot.value.orders) }
      // 订单恢复先由 Rust 重新查询 Binance 事实；前端只刷新展示，不触发重放下单。
      await refresh()
    }
  }

  async function recoverFundingAttempt(attemptId: string) {
    const attempt = await run('recover', () => api.recoverFundingAttempt(attemptId))
    if (attempt) {
      // 资金恢复会阻断后续自动资金动作，直到后端确认终态或人工解除 blocked。
      await refresh()
    }
  }

  async function resolveFundingAttempt(attemptId: string, resolution: FundingAttemptManualResolution) {
    const attempt = await run('recover', () =>
      api.resolveFundingAttempt({
        attemptId,
        resolution,
        confirmManual: true,
      }))
    if (attempt) {
      // 人工解除只更新恢复记录；真实 Simple Earn/Universal Transfer 请求不能由这里重发。
      await refresh()
    }
  }

  async function loadAdoptableOptionPositions() {
    const rows = await run('adopt', () => api.listAdoptableOptionPositions())
    if (rows) {
      adoptableOptionPositions.value = rows
    }
  }

  async function adoptOptionPositions(request: AdoptOptionPositionsRequest) {
    const orders = await run('adopt', () => api.adoptOptionPositions(request))
    if (orders && snapshot.value) {
      // 接管只把 Binance 当前义务仓转成本地跟踪订单，不混入 order_attempts 恢复记录。
      snapshot.value = { ...snapshot.value, orders: mergeOrders(orders, snapshot.value.orders) }
      adoptableOptionPositions.value = adoptableOptionPositions.value.map(row =>
        request.symbols.includes(row.symbol)
          ? { ...row, adoptable: false, alreadyTracked: true, reason: '本地已有跟踪记录' }
          : row,
      )
      await refresh()
    }
  }

  async function placeStrategy(request: StrategyRequest) {
    const flowId = request.clientFlowId ?? makeOrderFlowId()
    const orderRequest = { ...request, clientFlowId: flowId }
    startOrderProgress(flowId, orderRequest)
    const order = await run('order', () => api.placeManualStrategy(orderRequest))
    if (order && snapshot.value) {
      snapshot.value = { ...snapshot.value, orders: [order, ...snapshot.value.orders] }
      finishOrderProgressSuccess(flowId, order)
      await refresh()
      return
    }
    finishOrderProgressError(flowId, error.value ?? '下单失败，请查看错误提示和恢复记录')
    // 下单失败后的静默刷新用于暴露后端保存的恢复记录；不在前端重试真实订单。
    await refresh({ silent: true })
  }

  async function saveRule(rule: AutoInvestmentRule) {
    const next = await run('rule', () => api.saveAutoRule(rule))
    if (next && snapshot.value) {
      snapshot.value = { ...snapshot.value, autoRule: next }
    }
  }

  async function toggleAuto(request: ToggleAutoRequest) {
    const next = await run('rule', () => api.toggleAutoStrategy(request))
    patchSettings(next)
    if (snapshot.value?.autoRule) {
      // 前端先同步开关展示；二次确认、待恢复阻断和真实执行门禁都在 Rust 侧兜底。
      snapshot.value = {
        ...snapshot.value,
        autoRule: { ...snapshot.value.autoRule, enabled: request.enabled },
      }
    }
  }

  function patchSettings(next?: AppSettings) {
    if (next && snapshot.value) {
      snapshot.value = { ...snapshot.value, settings: next }
      reconcileAutoReconnectSettings(next)
    }
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

  /** 合并后端返回的更新订单，并按更新时间倒序展示。 */
  function mergeOrders(next: StrategyOrder[], current: StrategyOrder[]) {
    const byId = new Map(current.map(order => [order.id, order]))
    for (const order of next) {
      byId.set(order.id, order)
    }
    return [...byId.values()].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
  }

  return {
    snapshot,
    pending,
    loading,
    backgroundLoading,
    error,
    backgroundError,
    autoReconnectState,
    autoReconnectAttempts,
    autoReconnectDelayMs,
    autoReconnectPaused,
    autoReconnectMessage,
    canAutoReconnect,
    activeView,
    orderProgress,
    orderNotice,
    adoptableOptionPositions,
    settings,
    metrics,
    balances,
    candidates,
    scanDiagnostics,
    orders,
    pnl,
    pnlBreakdown,
    risk,
    autoRule,
    health,
    orderAttempts,
    fundingAttempts,
    pendingRecoveryCount,
    runningOrders,
    isLoading,
    isBusy,
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
    runSettlementScan,
    runAutoOnce,
    recoverAttempt,
    recoverFundingAttempt,
    resolveFundingAttempt,
    loadAdoptableOptionPositions,
    adoptOptionPositions,
    placeStrategy,
    saveRule,
    toggleAuto,
    openView,
    refreshViewData,
  }
})
