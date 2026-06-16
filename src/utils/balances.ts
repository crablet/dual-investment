// 账户资产展示标签。
//
// Rust 已按资产聚合金额和数量；这里只补充渲染 key 和账户来源文案。
import type { AssetBalance, BalanceRow } from '@/types/domain'
import { t } from '@/i18n'

import { isZeroDecimalString } from '@/utils/decimal'

interface BalanceDetail extends BalanceRow {
  key: string
}

interface AccountBalanceView extends AssetBalance {
  sourceLabel: string
  detailLabel: string
  details: BalanceDetail[]
}

export function presentAccountBalances(balances: AssetBalance[]): AccountBalanceView[] {
  return balances.map((balance) => {
    const details = balance.details.map((detail, index) => ({
      ...detail,
      key: `${detail.source}:${detail.asset}:${index}`,
    }))
    return {
      ...balance,
      details,
      sourceLabel: formatBalanceSource(details),
      detailLabel: details.map(formatBalanceDetailLabel).join(' · '),
    }
  })
}

function formatBalanceDetailLabel(detail: BalanceDetail): string {
  const locked = isZeroDecimalString(detail.locked) ? '' : t('ui.locked2', { p0: detail.locked })
  return t('ui.quantity2', { p0: t(`ui.${detail.source}`), p1: detail.asset, p2: detail.free, p3: locked })
}

function formatBalanceSource(details: BalanceDetail[]) {
  const uniqueSources = [...new Set(details.map(detail => detail.source))]
  if (uniqueSources.length === 1) {
    return t(`ui.${uniqueSources[0]}`)
  }
  return t('ui.accountSources', { p0: uniqueSources.length })
}
