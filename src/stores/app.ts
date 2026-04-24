// 应用级 Pinia store。
//
// 这里是前端唯一的跨页面状态源：保存 Tauri 快照、请求 loading/error、当前视图，以及
// 从快照派生出的指标和收益构成。真实业务计算仍以 Rust 返回的快照为准，前端只做展示层派生。

import { defineStore } from 'pinia'
import { computed, shallowRef } from 'vue'
import { api } from '@/services/api'
import type {
  AppSettings,
  AppSnapshot,
  AutoInvestmentRule,
  ConnectBinanceRequest,
  StrategyOrder,
  StrategyRequest,
  ToggleAutoRequest,
} from '@/types/domain'
import type { MetricCardModel, PnlBreakdownItem } from '@/types/view'
import {
  isNegativeDecimalString,
  isZeroDecimalString,
  negateDecimalString,
  sumDecimalStrings,
} from '@/utils/decimal'

type RequestKey =
  | 'snapshot'
  | 'connect'
  | 'credentials'
  | 'scan'
  | 'sync'
  | 'settlement'
  | 'auto'
  | 'recover'
  | 'order'
  | 'rule'

/** 请求包装选项。silent 请求用于后台轮询，不清空前台错误提示。 */
interface RunOptions {
  silent?: boolean
}

/** 应用内视图 ID；导航切换统一走 `openView`，以触发对应页面刷新。 */
export type AppViewId =
  | 'overview'
  | 'dual'
  | 'orders'
  | 'portfolio'
  | 'analytics'
  | 'risk'
  | 'settings'

const emptyPending = (): Record<RequestKey, number> => ({
  snapshot: 0,
  connect: 0,
  credentials: 0,
  scan: 0,
  sync: 0,
  settlement: 0,
  auto: 0,
  recover: 0,
  order: 0,
  rule: 0,
})

const metricMoneyFormatter = new Intl.NumberFormat('zh-CN', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
})

export const useAppStore = defineStore('app', () => {
  // 快照使用 shallowRef：后端每次返回完整对象，前端通过替换根引用触发更新。
  const snapshot = shallowRef<AppSnapshot>()
  const pending = shallowRef(emptyPending())
  const foregroundPending = shallowRef(0)
  const backgroundPending = shallowRef(0)
  const error = shallowRef<string>()
  const backgroundError = shallowRef<string>()
  const activeView = shallowRef<AppViewId>('overview')

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
  const orderAttempts = computed(() => snapshot.value?.orderAttempts ?? [])
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

  async function run<T>(
    key: RequestKey,
    task: () => Promise<T>,
    options: RunOptions = {},
  ): Promise<T | undefined> {
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

  async function refresh(options: RunOptions = {}) {
    const next = await run('snapshot', () => api.getAppSnapshot(), options)
    if (next) {
      snapshot.value = next
    }
  }

  /** 应用启动：先读取快照，再尝试用 keyring 凭据自动连接，但不自动开启策略。 */
  async function bootstrap() {
    await refresh()
    if (snapshot.value?.settings.savedCredentials && !snapshot.value.settings.connected) {
      await connectSaved()
    }
  }

  async function connect(request: ConnectBinanceRequest) {
    const next = await run('connect', () => api.connectBinance(request))
    patchSettings(next)
    if (next?.connected) {
      await syncLiveState()
    }
  }

  async function connectSaved() {
    const next = await run('connect', () => api.connectSavedBinance())
    patchSettings(next)
    if (next?.connected) {
      await syncLiveState()
    }
  }

  async function forgetCredentials() {
    const next = await run('credentials', () => api.forgetBinanceCredentials())
    patchSettings(next)
    if (next) {
      await refresh()
    }
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
      await refresh()
    }
  }

  async function placeStrategy(request: StrategyRequest) {
    const order = await run('order', () => api.placeManualStrategy(request))
    if (order && snapshot.value) {
      snapshot.value = { ...snapshot.value, orders: [order, ...snapshot.value.orders] }
      await refresh()
    }
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
      snapshot.value = {
        ...snapshot.value,
        autoRule: { ...snapshot.value.autoRule, enabled: request.enabled },
      }
    }
  }

  function patchSettings(next?: AppSettings) {
    if (next && snapshot.value) {
      snapshot.value = { ...snapshot.value, settings: next }
    }
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
    activeView,
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
    orderAttempts,
    runningOrders,
    isLoading,
    isBusy,
    bootstrap,
    refresh,
    connect,
    connectSaved,
    forgetCredentials,
    scan,
    syncLiveState,
    runSettlementScan,
    runAutoOnce,
    recoverAttempt,
    placeStrategy,
    saveRule,
    toggleAuto,
    openView,
    refreshViewData,
  }
})

function buildMetrics(
  settings: AppSettings | undefined,
  balances: AppSnapshot['balances'],
  pnl: AppSnapshot['pnl'] | undefined,
): MetricCardModel[] {
  const connected = settings?.connected ?? false
  const syncState = connected ? 'Binance 账户已同步' : '等待 API 连接'
  const totalEquity = sumDecimalStrings(balances.map(balance => balance.usdtValue))
  const availableMargin = balances.find(
    balance => balance.asset === 'USDT' && balance.source === '期权保证金',
  )?.free ?? '0'
  const todayPremium = pnl?.today ?? '0'

  return [
    {
      id: 'portfolio',
      title: '总资产估值',
      value: formatMetricMoney(totalEquity),
      subValue: syncState,
      change: 'Binance',
      icon: 'wallet',
      tone: 'gold',
    },
    {
      id: 'margin',
      title: '可用保证金',
      value: formatMetricMoney(availableMargin),
      subValue: syncState,
      change: 'Binance',
      icon: 'shield',
      tone: 'blue',
    },
    {
      id: 'premium',
      title: '今日权利金',
      value: formatMetricMoney(todayPremium),
      subValue: '今日',
      change: '订单',
      icon: 'coins',
      tone: 'gold',
    },
    {
      id: 'status',
      title: '运行状态',
      value: settings?.autoEnabled ? '运行中' : connected ? '已连接' : '未连接',
      subValue: 'Tauri 客户端',
      change: syncState,
      icon: 'activity',
      tone: settings?.autoEnabled ? 'green' : connected ? 'gold' : 'gray',
    },
  ]
}

/** 从收益报告派生图表构成。负数手续费在前端作为支出显示。 */
function buildPnlBreakdown(report: AppSnapshot['pnl'] | undefined): PnlBreakdownItem[] {
  if (!report || (isZeroDecimalString(report.cumulative) && isZeroDecimalString(report.optionPremium))) {
    return []
  }

  return [
    {
      label: '期权权利金',
      value: report.optionPremium,
      color: '#d89b2b',
    },
    {
      label: '手续费',
      value: negateDecimalString(report.fees),
      color: '#cc4b37',
    },
    {
      label: '现货补齐浮动盈亏',
      value: report.floatingSpot,
      color: isNegativeDecimalString(report.floatingSpot) ? '#cc4b37' : '#2fa66a',
    },
  ]
}

function formatMetricMoney(value: string) {
  const numeric = Number(value)
  if (!Number.isFinite(numeric)) {
    return `${value} USDT`
  }
  return `${metricMoneyFormatter.format(numeric)} USDT`
}
