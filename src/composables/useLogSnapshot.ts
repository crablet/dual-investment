// 设置页文件日志状态。
//
// 日志读取和文件操作都通过 Tauri command 进入 Rust，不提供浏览器 fallback。

import type { LogSnapshot } from '@/types/domain'
import { computed, onMounted, shallowRef } from 'vue'
import { api } from '@/services/api'

type LogAction = 'refresh' | 'openFile' | 'openFolder' | 'clear'

export function useLogSnapshot() {
  const snapshot = shallowRef<LogSnapshot>()
  const pendingAction = shallowRef<LogAction>()
  const error = shallowRef<string>()

  const hasFiles = computed(() => snapshot.value?.hasFiles ?? false)
  const recentEntries = computed(() => snapshot.value?.recentEntries ?? [])

  /**
   * 包装日志相关 Tauri command。
   *
   * 日志操作属于设置页局部状态，失败只写入本 composable 的 error，不覆盖全局交易错误。
   */
  async function run<T>(action: LogAction, task: () => Promise<T>) {
    pendingAction.value = action
    error.value = undefined
    try {
      return await task()
    }
    catch (caught) {
      error.value = caught instanceof Error ? caught.message : String(caught)
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
    refresh,
    openCurrentFile,
    openFolder,
    clear,
  }
}
