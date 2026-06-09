// 账户资产展示标签。
//
// Rust 已按资产聚合金额和数量；这里只补充渲染 key 和账户来源文案。
import type { AssetBalance, BalanceRow } from '@/types/domain'
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
      detailLabel: details.map(formatBalanceDetailLabel).join('；'),
    }
  })
}

function formatBalanceDetailLabel(detail: BalanceDetail): string {
  const locked = isZeroDecimalString(detail.locked) ? '' : `，锁定 ${detail.locked}`
  return `${detail.source} ${detail.asset}：数量 ${detail.free}${locked}`
}

function formatBalanceSource(details: BalanceDetail[]) {
  const uniqueSources = [...new Set(details.map(detail => detail.source))]
  if (uniqueSources.length === 1) {
    return uniqueSources[0]
  }
  return `${uniqueSources.length} 个账户来源`
}
