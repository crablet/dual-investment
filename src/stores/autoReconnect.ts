// 已保存凭据自动重连的退避策略。
//
// store 只使用这些纯函数计算和显示下一次重连时间。重连资格直接来自 Rust 错误字段。

export const reconnectBackoffDelays = [5_000, 15_000, 30_000, 60_000, 120_000, 300_000]

/** 根据失败次数取得下一次重连延迟；超过表尾后保持最大退避时间。 */
export function reconnectDelayAfterAttempt(attempts: number) {
  return reconnectBackoffDelays[Math.min(attempts, reconnectBackoffDelays.length - 1)]
}

/** 把毫秒延迟转成前端提示文案，避免各处重复拼接中文单位。 */
export function formatReconnectDelay(ms: number) {
  const seconds = Math.max(1, Math.round(ms / 1_000))
  if (seconds < 60) {
    return `${seconds} 秒`
  }
  return `${Math.round(seconds / 60)} 分钟`
}
