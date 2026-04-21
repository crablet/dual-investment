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

interface RunOptions {
  silent?: boolean
}

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

export const useAppStore = defineStore('app', () => {
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
  const metrics = computed(() => snapshot.value?.metrics ?? [])
  const balances = computed(() => snapshot.value?.balances ?? [])
  const candidates = computed(() => snapshot.value?.candidates ?? [])
  const scanDiagnostics = computed(() => snapshot.value?.scanDiagnostics)
  const orders = computed(() => snapshot.value?.orders ?? [])
  const pnl = computed(() => snapshot.value?.pnl)
  const risk = computed(() => snapshot.value?.risk)
  const autoRule = computed(() => snapshot.value?.autoRule)
  const auditRecent = computed(() => snapshot.value?.auditRecent ?? [])
  const orderAttempts = computed(() => snapshot.value?.orderAttempts ?? [])
  const runningOrders = computed(() =>
    orders.value.filter(order => order.status === 'running' || order.status === 'partiallyFilled'),
  )

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
    risk,
    autoRule,
    auditRecent,
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
