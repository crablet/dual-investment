import type { AppSettings, AppSnapshot } from '@/types/domain'
import type { MetricCardModel, PnlBreakdownItem } from '@/types/view'
import {
  isZeroDecimalString,
  sumDecimalStrings,
} from '@/utils/decimal'

const metricMoneyFormatter = new Intl.NumberFormat('zh-CN', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
})

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
      value: settings?.autoEnabled ? '运行中' : connected ? '已连接' : '未连接',
      subValue: 'Tauri 客户端',
      change: syncState,
      icon: 'activity',
      tone: settings?.autoEnabled ? 'green' : connected ? 'gold' : 'gray',
    },
  ]
}

/** 从收益报告派生图表构成。 */
export function buildPnlBreakdown(report: AppSnapshot['pnl'] | undefined): PnlBreakdownItem[] {
  if (!report || (isZeroDecimalString(report.cumulative) && isZeroDecimalString(report.optionPremium))) {
    return []
  }

  return [
    {
      key: 'option-premium',
      label: '期权权利金',
      value: report.optionPremium,
      color: '#d89b2b',
    },
    {
      key: 'floating-spot',
      label: '现货补齐浮动盈亏',
      value: report.floatingSpot,
      color: '#5f8ff5',
    },
  ]
}

function formatMetricMoney(value: string) {
  const numeric = Number(value)
  if (!Number.isFinite(numeric)) {
    return `${value} USDT`
  }
  return `${metricMoneyFormatter.format(numeric)} USDT`
}
