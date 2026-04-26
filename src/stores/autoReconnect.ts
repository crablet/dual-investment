export const reconnectBackoffDelays = [5_000, 15_000, 30_000, 60_000, 120_000, 300_000]

export function reconnectDelayAfterAttempt(attempts: number) {
  return reconnectBackoffDelays[Math.min(attempts, reconnectBackoffDelays.length - 1)]
}

export function formatReconnectDelay(ms: number) {
  const seconds = Math.max(1, Math.round(ms / 1_000))
  if (seconds < 60) {
    return `${seconds} 秒`
  }
  return `${Math.round(seconds / 60)} 分钟`
}

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
