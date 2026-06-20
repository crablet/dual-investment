// 总览金额只展示后端快照；账户尚未查询时保留占位。
import type { AccountSnapshot, PnlReport } from '@/types/domain'
import type { MetricCardModel } from '@/types/view'
import { useFormat } from '@/composables/useFormat'
import { t } from '@/i18n'

export function buildMetrics(account: AccountSnapshot | undefined, pnl: PnlReport | undefined): MetricCardModel[] {
  const { decimal } = useFormat()
  return [
    { id: 'portfolio', title: t('ui.totalAssetValue'), value: account?.updatedAt ? decimal(account.totalUsdtValue) : '--' },
    { id: 'margin', title: t('ui.availableMargin'), value: account?.updatedAt ? decimal(account.availableMargin) : '--' },
    { id: 'premium', title: t('ui.todayRecordedPremium'), value: pnl ? decimal(pnl.todayRecordedPremium) : '--' },
  ]
}
