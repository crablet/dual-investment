// 计算重连间隔和提示时间。是否允许重连由 Rust 错误字段决定。

import { t } from '@/i18n'

export const reconnectBackoffDelays = [5_000, 15_000, 30_000, 60_000, 120_000, 300_000]

/** 根据失败次数取得下一次重连延迟；超过表尾后保持最大退避时间。 */
export function reconnectDelayAfterAttempt(attempts: number) {
  return reconnectBackoffDelays[Math.min(attempts, reconnectBackoffDelays.length - 1)]
}

/** 把毫秒延迟转成前端提示文案，统一按所选语言显示时间单位。 */
export function formatReconnectDelay(ms: number) {
  const seconds = Math.max(1, Math.round(ms / 1_000))
  if (seconds < 60) {
    return t('ui.seconds', { p0: seconds })
  }
  return t('ui.minutes', { p0: Math.round(seconds / 60) })
}
