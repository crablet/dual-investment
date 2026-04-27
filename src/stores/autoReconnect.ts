// 已保存凭据自动重连的退避策略。
//
// store 只使用这些纯函数判断下一次重连时间和错误是否需要人工介入。这里不直接触发
// Tauri command，避免计时策略和真实连接副作用耦合。

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

/**
 * 判断保存凭据重连是否应该暂停。
 *
 * 这些错误通常来自缺凭据、签名失败、权限不足或账户不可交易，继续静默重试不会恢复，
 * 需要用户检查 Binance API 设置后再手动连接。
 */
export function isPermanentSavedConnectionError(message: string) {
  return [
    '系统 keyring 中没有已保存的 Binance API 凭据',
    'Binance 期权账户当前不可交易',
    'Binance 现货账户当前不可交易',
    'HTTP 401',
    'code -2014',
    'code -2015',
    'code -1022',
  ].some(pattern => message.includes(pattern))
}
