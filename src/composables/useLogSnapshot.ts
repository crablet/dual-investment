// 通过 Tauri 管理设置页日志状态和文件操作。

import type { LocalizedMessage } from '@/i18n'
import type { LogSnapshot } from '@/types/domain'
import { computed, onBeforeUnmount, onMounted, shallowRef } from 'vue'
import { api, commandError } from '@/services/api'

type LogAction = 'openFile' | 'openFolder' | 'clear'
const logRefreshIntervalMs = 3_000

export function useLogSnapshot() {
  const snapshot = shallowRef<LogSnapshot>()
  const pendingAction = shallowRef<LogAction>()
  const actionError = shallowRef<LocalizedMessage>()
  const readError = shallowRef<LocalizedMessage>()
  const error = computed(() => actionError.value ?? readError.value)
  let refreshTimer: number | undefined
  let reading = false
  let revision = 0

  const hasFiles = computed(() => snapshot.value?.hasFiles ?? false)
  const recentEntries = computed(() => snapshot.value?.recentEntries ?? [])
  const writeError = computed(() => snapshot.value?.writeError)

  async function refresh() {
    if (refreshTimer === undefined || pendingAction.value || reading)
      return
    const requestedRevision = revision
    reading = true
    try {
      const next = await api.getLogSnapshot()
      if (revision === requestedRevision) {
        snapshot.value = next
        readError.value = undefined
      }
    }
    catch (caught) {
      if (revision === requestedRevision)
        readError.value = commandError(caught).message
    }
    finally {
      reading = false
    }
  }

  async function run(action: LogAction, task: () => Promise<LogSnapshot | void>) {
    if (pendingAction.value)
      return
    // 文件操作前发出的查询可能晚于删除返回，不能用旧快照恢复已删除的日志。
    revision++
    pendingAction.value = action
    actionError.value = undefined
    try {
      const next = await task()
      if (next) {
        snapshot.value = next
        readError.value = undefined
      }
    }
    catch (caught) {
      actionError.value = commandError(caught).message
    }
    finally {
      pendingAction.value = undefined
    }
  }

  async function openCurrentFile() {
    // 打开外部日志查看器后刷新快照，让删除按钮和最近日志跟随文件系统实际状态。
    await run('openFile', api.openCurrentLogFile)
    await refresh()
  }

  function stopPolling() {
    window.clearInterval(refreshTimer)
    refreshTimer = undefined
  }

  function updatePolling() {
    stopPolling()
    if (!document.hidden) {
      refreshTimer = window.setInterval(refresh, logRefreshIntervalMs)
      void refresh()
    }
  }

  onMounted(() => {
    document.addEventListener('visibilitychange', updatePolling)
    updatePolling()
  })
  onBeforeUnmount(() => {
    document.removeEventListener('visibilitychange', updatePolling)
    stopPolling()
    revision++
  })

  return {
    hasFiles,
    recentEntries,
    pendingAction,
    error,
    writeError,
    openCurrentFile,
    openFolder: () => run('openFolder', api.openLogFolder),
    clear: () => run('clear', api.clearLogFiles),
  }
}
