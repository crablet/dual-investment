// 总览指标的展示派生。
//
// 这里只把后端快照整理成指标卡模型，不做真实金融决策。金额求和使用 Decimal
// 字符串工具，避免为了 UI 汇总先转成 JavaScript number。

import type { AppSettings, AppSnapshot } from '@/types/domain'
import type { MetricCardModel } from '@/types/view'
import {
  decimalStringToDisplayNumber,
  sumDecimalStrings,
} from '@/utils/decimal'

const metricMoneyFormatter = new Intl.NumberFormat('zh-CN', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
})

/** 根据连接状态、账户资产和收益快照构造总览顶部指标卡。 */
export function buildMetrics(
  settings: AppSettings | undefined,
  balances: AppSnapshot['balances'],
  pnl: AppSnapshot['pnl'] | undefined,
): MetricCardModel[] {
  const connected = settings?.connected ?? false
  const syncState = connected ? 'Binance 账户已同步' : '等待 API 连接'
  const totalEquity = sumDecimalStrings(balances.map(balance => balance.usdtValue))
  const availableMargin = balances.find(
    balance => balance.asset === 'USDT' && balance.source === '期权保证金',
  )?.free ?? '0'
  const todayPremium = pnl?.today ?? '0'

  return [
    {
      id: 'portfolio',
      title: '总资产估值',
      value: formatMetricMoney(totalEquity),
      subValue: syncState,
      change: 'Binance',
      icon: 'wallet',
      tone: 'gold',
    },
    {
      id: 'margin',
      title: '可用保证金',
      value: formatMetricMoney(availableMargin),
      subValue: syncState,
      change: 'Binance',
      icon: 'shield',
      tone: 'blue',
    },
    {
      id: 'premium',
      title: '今日权利金',
      value: formatMetricMoney(todayPremium),
      subValue: '今日',
      change: '订单',
      icon: 'coins',
      tone: 'gold',
    },
    {
      id: 'status',
      title: '运行状态',
      value: connected ? '已连接' : '未连接',
      subValue: 'Tauri 客户端',
      change: syncState,
      icon: 'activity',
      tone: connected ? 'gold' : 'gray',
    },
  ]
}

function formatMetricMoney(value: string) {
  // 指标卡格式化允许转 number；汇总本身已经由 Decimal 字符串工具完成。
  const numeric = decimalStringToDisplayNumber(value)
  if (!Number.isFinite(numeric)) {
    return `${value} USDT`
  }
  return `${metricMoneyFormatter.format(numeric)} USDT`
}
