// 为日历和“今日”摘要提供会随本地日期更新的状态。
// 跨午夜、从休眠恢复或重新聚焦时重新核对日期，不依赖业务快照刷新。

import { computed, onBeforeUnmount, onMounted, readonly, shallowRef } from 'vue'
import { formatDateKey, parseDateKey } from '@/utils/date'

export function useLocalToday() {
  const todayKey = shallowRef(formatDateKey(new Date()))
  const today = computed(() => parseDateKey(todayKey.value))
  let timer: number | undefined

  function refreshToday() {
    if (timer !== undefined) {
      window.clearTimeout(timer)
    }
    const now = new Date()
    todayKey.value = formatDateKey(now)
    const nextDay = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1)
    // 本地午夜不一定相隔 24 小时；定期重查也能跟上系统时钟和时区调整。
    timer = window.setTimeout(refreshToday, Math.min(nextDay.getTime() - now.getTime(), 60_000))
  }

  onMounted(() => {
    refreshToday()
    window.addEventListener('focus', refreshToday)
    document.addEventListener('visibilitychange', refreshToday)
  })

  onBeforeUnmount(() => {
    window.clearTimeout(timer)
    window.removeEventListener('focus', refreshToday)
    document.removeEventListener('visibilitychange', refreshToday)
  })

  return { today, todayKey: readonly(todayKey) }
}
