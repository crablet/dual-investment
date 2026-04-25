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

  const loading = computed(() => pendingAction.value !== undefined)
  const files = computed(() => snapshot.value?.files ?? [])
  const recentEntries = computed(() => snapshot.value?.recentEntries ?? [])
  const fileCountLabel = computed(() => `${files.value.length} 个文件`)
  const currentFileName = computed(() => {
    const currentFile = snapshot.value?.currentFile
    if (!currentFile) {
      return '暂无日志文件'
    }
    return currentFile.split(/[\\/]/).at(-1) ?? currentFile
  })

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
    snapshot,
    files,
    recentEntries,
    fileCountLabel,
    currentFileName,
    loading,
    pendingAction,
    error,
    refresh,
    openCurrentFile,
    openFolder,
    clear,
  }
}
