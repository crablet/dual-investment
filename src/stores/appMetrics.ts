// 整理 Rust 快照中的总览指标；尚未查询的账户显示未知，不能显示为零余额。

import type { AccountSnapshot, AppSettings, PnlReport } from '@/types/domain'
import type { MetricCardModel } from '@/types/view'
import { formatLocale, t } from '@/i18n'
import { decimalStringToDisplayNumber } from '@/utils/decimal'

function metricMoneyFormatter() {
  return new Intl.NumberFormat(formatLocale(), {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })
}

/** 根据连接状态、账户资产和收益快照构造总览顶部指标卡。 */
export function buildMetrics(
  settings: AppSettings | undefined,
  account: AccountSnapshot | undefined,
  pnl: PnlReport | undefined,
): MetricCardModel[] {
  const connected = settings?.connected ?? false
  const totalEquity = account?.updatedAt ? account.totalUsdtValue : undefined
  const availableMargin = account?.updatedAt ? account.availableMargin : undefined

  return [
    {
      id: 'portfolio',
      title: t('ui.totalAssetValue'),
      value: formatMetricMoney(totalEquity),
      icon: 'wallet',
      tone: 'gold',
    },
    {
      id: 'margin',
      title: t('ui.availableMargin'),
      value: formatMetricMoney(availableMargin),
      icon: 'shield',
      tone: 'blue',
    },
    {
      id: 'premium',
      title: t('ui.todayRecordedPremium'),
      value: formatMetricMoney(pnl?.todayRecordedPremium),
      icon: 'coins',
      tone: 'gold',
    },
    {
      id: 'status',
      title: t('ui.runtimeStatus'),
      value: connected ? t('ui.connected') : t('ui.disconnected'),
      icon: 'activity',
      tone: connected ? 'gold' : 'gray',
    },
  ]
}

function formatMetricMoney(value: string | undefined) {
  if (value === undefined) {
    return '-- USDT'
  }
  const numeric = decimalStringToDisplayNumber(value)
  return `${metricMoneyFormatter().format(numeric)} USDT`
}
