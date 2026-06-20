// 通过 Tauri 管理设置页日志状态和文件操作。

import type { LocalizedMessage } from '@/i18n'
import type { LogSnapshot } from '@/types/domain'
import { computed, onMounted, shallowRef } from 'vue'
import { api, commandError } from '@/services/api'

type LogAction = 'refresh' | 'openFile' | 'openFolder' | 'clear'

export function useLogSnapshot() {
  const snapshot = shallowRef<LogSnapshot>()
  const pendingAction = shallowRef<LogAction>()
  const error = shallowRef<LocalizedMessage>()

  const hasFiles = computed(() => snapshot.value?.hasFiles ?? false)
  const recentEntries = computed(() => snapshot.value?.recentEntries ?? [])
  const writeError = computed(() => snapshot.value?.writeError)

  /** 日志错误只显示在设置页，不覆盖全局交易错误。 */
  async function run<T>(action: LogAction, task: () => Promise<T>) {
    pendingAction.value = action
    error.value = undefined
    try {
      return await task()
    }
    catch (caught) {
      error.value = commandError(caught).message
      return undefined
    }
    finally {
      pendingAction.value = undefined
    }
  }

  async function refresh() {
    const next = await run('refresh', () => api.getLogSnapshot())
    if (next) {
      snapshot.value = next
    }
  }

  async function openCurrentFile() {
    // 打开外部日志查看器后刷新快照，让删除按钮和最近日志跟随文件系统实际状态。
    await run('openFile', () => api.openCurrentLogFile())
    await refresh()
  }

  async function openFolder() {
    await run('openFolder', () => api.openLogFolder())
  }

  async function clear() {
    const next = await run('clear', () => api.clearLogFiles())
    if (next) {
      snapshot.value = next
    }
  }

  onMounted(refresh)

  return {
    hasFiles,
    recentEntries,
    pendingAction,
    error,
    writeError,
    refresh,
    openCurrentFile,
    openFolder,
    clear,
  }
}
