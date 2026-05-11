// 应用后台同步调度器。
//
// 前端只负责定时调用快照、实盘同步和已保存凭据重连，不创建新的期权策略订单。
// Rust 的实盘同步会推进既有恢复、结算、现货补齐和资金收尾，真实动作仍由后端门禁决定。

import { onBeforeUnmount, onMounted } from 'vue'
import { useAppStore } from '@/stores/app'

const reconnectProbeDelay = 5_000
const baseSyncDelay = 60_000
const maxSyncDelay = 300_000

/** 绑定订单进度事件并启动后台静默同步。 */
export function useBackgroundSync() {
  const store = useAppStore()
  let syncTimer: number | undefined
  let syncDelay = baseSyncDelay
  let syncDisposed = false

  onMounted(() => {
    syncDisposed = false
    void store.bindOrderProgressEvents()
    void store.bootstrap()
    scheduleBackgroundSync(reconnectProbeDelay)
  })

  onBeforeUnmount(() => {
    syncDisposed = true
    store.disposeOrderProgressEvents()
    if (syncTimer !== undefined) {
      window.clearTimeout(syncTimer)
    }
  })

  function scheduleBackgroundSync(delay = syncDelay) {
    if (syncDisposed) {
      return
    }
    syncTimer = window.setTimeout(() => {
      void runBackgroundSync()
    }, delay)
  }

  async function runBackgroundSync() {
    syncTimer = undefined
    if (store.isLoading('snapshot') || store.isLoading('connect')) {
      syncDelay = reconnectProbeDelay
    }
    else if (store.settings?.connected && !store.isLoading('sync')) {
      await store.syncLiveState({ silent: true })
      syncDelay = store.backgroundError
        ? Math.min(syncDelay * 2, maxSyncDelay)
        : baseSyncDelay
    }
    else if (!store.settings?.connected) {
      await store.refresh({ silent: true })
      if (store.canAutoReconnect) {
        const connected = await store.tryAutoReconnectSaved()
        syncDelay = connected ? baseSyncDelay : store.autoReconnectDelayMs
      }
      else {
        syncDelay = baseSyncDelay
      }
    }
    scheduleBackgroundSync()
  }
}
