import { defineStore } from 'pinia'
import { computed, shallowRef } from 'vue'
import { api } from '@/services/api'
import type {
  AppSettings,
  AppSnapshot,
  AutoInvestmentRule,
  ConnectBinanceRequest,
  StrategyRequest,
  ToggleAutoRequest,
} from '@/types/domain'

export const useAppStore = defineStore('app', () => {
  const snapshot = shallowRef<AppSnapshot>()
  const loading = shallowRef(false)
  const error = shallowRef<string>()
  const activeView = shallowRef('overview')

  const settings = computed(() => snapshot.value?.settings)
  const metrics = computed(() => snapshot.value?.metrics ?? [])
  const balances = computed(() => snapshot.value?.balances ?? [])
  const candidates = computed(() => snapshot.value?.candidates ?? [])
  const orders = computed(() => snapshot.value?.orders ?? [])
  const pnl = computed(() => snapshot.value?.pnl)
  const risk = computed(() => snapshot.value?.risk)
  const autoRule = computed(() => snapshot.value?.autoRule)
  const auditRecent = computed(() => snapshot.value?.auditRecent ?? [])
  const runningOrders = computed(() => orders.value.filter(order => order.status === 'running'))

  async function run<T>(task: () => Promise<T>): Promise<T | undefined> {
    loading.value = true
    error.value = undefined
    try {
      return await task()
    }
    catch (caught) {
      error.value = caught instanceof Error ? caught.message : String(caught)
      return undefined
    }
    finally {
      loading.value = false
    }
  }

  async function refresh() {
    const next = await run(() => api.getAppSnapshot())
    if (next) {
      snapshot.value = next
    }
  }

  async function connect(request: ConnectBinanceRequest) {
    const next = await run(() => api.connectBinance(request))
    patchSettings(next)
  }

  async function scan() {
    const next = await run(() => api.scanCandidates())
    if (next && snapshot.value) {
      snapshot.value = { ...snapshot.value, candidates: next }
    }
  }

  async function placeStrategy(request: StrategyRequest) {
    const order = await run(() => api.placeManualStrategy(request))
    if (order && snapshot.value) {
      snapshot.value = { ...snapshot.value, orders: [order, ...snapshot.value.orders] }
    }
  }

  async function saveRule(rule: AutoInvestmentRule) {
    const next = await run(() => api.saveAutoRule(rule))
    if (next && snapshot.value) {
      snapshot.value = { ...snapshot.value, autoRule: next }
    }
  }

  async function toggleAuto(request: ToggleAutoRequest) {
    const next = await run(() => api.toggleAutoStrategy(request))
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

  return {
    snapshot,
    loading,
    error,
    activeView,
    settings,
    metrics,
    balances,
    candidates,
    orders,
    pnl,
    risk,
    autoRule,
    auditRecent,
    runningOrders,
    refresh,
    connect,
    scan,
    placeStrategy,
    saveRule,
    toggleAuto,
  }
})
