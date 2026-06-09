// 应用后台同步调度器。
//
// 前端定时读取 Rust 的本地快照并按后端许可重连。交易所周期同步只有 Rust 维护任务执行，
// 避免隐藏窗口和页面导航重复查询账户、订单。

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
    else {
      await store.refresh({ silent: true })
      if (store.canAutoReconnect) {
        const connected = await store.tryAutoReconnectSaved()
        syncDelay = connected ? baseSyncDelay : store.autoReconnectDelayMs
      }
      else {
        syncDelay = store.backgroundError
          ? Math.min(syncDelay * 2, maxSyncDelay)
          : baseSyncDelay
      }
    }
    scheduleBackgroundSync()
  }
}
