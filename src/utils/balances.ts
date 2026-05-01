// 账户资产展示聚合工具。
//
// 同名资产可能同时来自现货、期权保证金和 Simple Earn。这里只做前端展示聚合，
// 不产生新的交易事实；金额和数量始终以 Decimal 字符串求和，避免 JS number 精度问题。
import type { BalanceRow, DecimalString } from '@/types/domain'
import { isZeroDecimalString, sumDecimalStrings } from '@/utils/decimal'

export interface BalanceDetail {
  key: string
  asset: string
  source: string
  free: DecimalString
  locked: DecimalString
  usdtValue: DecimalString
  usdtValueKnown: boolean
}

export interface AggregatedBalance {
  asset: string
  free: DecimalString
  locked: DecimalString
  usdtValue: DecimalString
  usdtValueKnown: boolean
  sourceLabel: string
  detailLabel: string
  details: BalanceDetail[]
}

export function aggregateBalances(balances: BalanceRow[]): AggregatedBalance[] {
  const grouped = new Map<string, BalanceDetail[]>()
  for (const [index, balance] of balances.entries()) {
    const asset = normalizeAsset(balance.asset)
    const details = grouped.get(asset) ?? []
    details.push({
      key: `${balance.source || '账户'}:${asset}:${index}`,
      asset,
      source: balance.source || '账户',
      free: balance.free,
      locked: balance.locked,
      usdtValue: balance.usdtValue,
      usdtValueKnown: balance.usdtValueKnown,
    })
    grouped.set(asset, details)
  }

  return [...grouped.entries()].map(([asset, details]) => ({
    asset,
    details,
    free: sumDecimalStrings(details.map(detail => detail.free)),
    locked: sumDecimalStrings(details.map(detail => detail.locked)),
    usdtValue: sumDecimalStrings(details.map(detail => detail.usdtValue)),
    usdtValueKnown: details.every(detail => detail.usdtValueKnown),
    sourceLabel: formatAggregatedBalanceSource(details),
    detailLabel: details.map(formatBalanceDetailLabel).join('；'),
  }))
}

export function formatBalanceDetailLabel(detail: BalanceDetail): string {
  const locked = isZeroDecimalString(detail.locked) ? '' : `，锁定 ${detail.locked}`
  return `${detail.source} ${detail.asset}：数量 ${detail.free}${locked}`
}

function normalizeAsset(asset: string) {
  return asset.trim().toUpperCase()
}

function formatAggregatedBalanceSource(details: BalanceDetail[]) {
  const uniqueSources = [...new Set(details.map(detail => detail.source))]
  if (uniqueSources.length === 1) {
    return uniqueSources[0]
  }
  return `${uniqueSources.length} 个账户来源`
}
