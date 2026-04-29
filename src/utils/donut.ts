// 环形图纯派生逻辑。
//
// 输入仍是前端展示模型；这里只做稳定排序、稳定取色和 SVG 百分比派生，不参与
// 真实收益归因或金融事实计算。

import type { PnlBreakdownItem } from '@/types/view'
import {
  decimalStringToDisplayNumber,
  isNegativeDecimalString,
  isZeroDecimalString,
  sumDecimalStrings,
} from '@/utils/decimal'

export interface NormalizedDonutItem extends PnlBreakdownItem {
  key: string
  colorKey: string
  sortKey: string
  sourceIndex: number
}

export interface DonutSegment extends NormalizedDonutItem {
  color: string
  percentLabel: string
  valueLabel: string
  dasharray: string
  dashoffset: string
  negative: boolean
  ariaLabel: string
}

export interface DonutCenterDisplay {
  label: string
  value: string
  meta: string
  negative: boolean
}

// 业务上有稳定含义的标签固定顺序，避免接口返回顺序变化导致扇区位置跳动。
const stableLabelOrder = new Map<string, number>([
  ['期权权利金', 10],
  ['现货补齐浮动盈亏', 20],
  ['USDT', 100],
  ['USDC', 110],
  ['BTC', 120],
  ['BNB', 130],
  ['BNSOL', 140],
  ['TON', 150],
  ['WBETH', 160],
])

const fixedColors = new Map<string, string>([
  ['期权权利金', '#d89b2b'],
  ['现货补齐浮动盈亏', '#5f8ff5'],
  ['USDT', '#f5b625'],
  ['USDC', '#2775ca'],
  ['BTC', '#f7931a'],
  ['BNB', '#f0b90b'],
  ['BNSOL', '#52c66f'],
  ['TON', '#0098ea'],
  ['WBETH', '#8e70ff'],
])

const fallbackPalette = [
  '#5f8ff5',
  '#52c66f',
  '#d89b2b',
  '#8e70ff',
  '#24b3a8',
  '#ef7f45',
  '#6ac7ff',
  '#c284ff',
]

const collator = new Intl.Collator('zh-CN', {
  numeric: true,
  sensitivity: 'base',
})

const valueFormatter = new Intl.NumberFormat('zh-CN', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
})

const percentFormatter = new Intl.NumberFormat('zh-CN', {
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
})

export function normalizeDonutItems(items: PnlBreakdownItem[]): NormalizedDonutItem[] {
  const keyCounts = new Map<string, number>()

  return items
    .map((item, index): NormalizedDonutItem => {
      const baseKey = item.key ?? item.label
      const occurrence = keyCounts.get(baseKey) ?? 0
      keyCounts.set(baseKey, occurrence + 1)
      return {
        ...item,
        // 渲染 key 必须唯一，但颜色和排序仍使用业务 key，避免同资产拆分时颜色漂移。
        key: occurrence === 0 ? baseKey : `${baseKey}#${occurrence + 1}`,
        colorKey: baseKey,
        sortKey: baseKey,
        sourceIndex: index,
      }
    })
    .filter(item => !isZeroDecimalString(item.value))
    .sort((a, b) => {
      const rankDiff = sortRank(a) - sortRank(b)
      if (rankDiff !== 0) {
        return rankDiff
      }
      const keyCompare = collator.compare(a.key, b.key)
      return keyCompare === 0 ? a.sourceIndex - b.sourceIndex : keyCompare
    })
}

export function absoluteDonutTotal(items: NormalizedDonutItem[]) {
  // 饼图用绝对值计算占比，负收益仍以颜色/文案标识，不让负数抵消总占比。
  return items.reduce((sum, item) => sum + Math.abs(decimalStringToDisplayNumber(item.value)), 0)
}

export function buildDonutSegments(items: NormalizedDonutItem[], unit: string): DonutSegment[] {
  const total = absoluteDonutTotal(items)
  let offset = 25
  if (total <= 0) {
    return []
  }

  return items.map((item) => {
    const numericValue = decimalStringToDisplayNumber(item.value)
    const length = (Math.abs(numericValue) / total) * 100
    const percent = (Math.abs(numericValue) / total) * 100
    const segment = {
      ...item,
      color: resolveColor(item),
      percentLabel: `${percentFormatter.format(percent)}%`,
      valueLabel: formatDonutValue(item.value),
      dasharray: `${svgNumber(length)} ${svgNumber(100 - length)}`,
      dashoffset: svgNumber(offset),
      negative: isNegativeDecimalString(item.value),
      ariaLabel: `${item.label}，${formatDonutValue(item.value)} ${unit}，占比 ${percentFormatter.format(percent)}%`,
    }
    offset -= length
    return segment
  })
}

export function resolveCenterValue(
  items: NormalizedDonutItem[],
  center?: string,
  centerValue?: string,
) {
  return centerValue ?? center ?? sumDecimalStrings(items.map(item => item.value))
}

export function centerDisplayForSegment(
  active: DonutSegment | undefined,
  centerLabel: string,
  centerValue: string,
  unit: string,
): DonutCenterDisplay {
  if (active) {
    return {
      label: active.label,
      value: active.valueLabel,
      meta: active.percentLabel,
      negative: active.negative,
    }
  }
  return {
    label: centerLabel,
    value: formatDonutValue(centerValue),
    meta: unit,
    negative: isNegativeDecimalString(centerValue),
  }
}

export function formatDonutValue(value: string) {
  const numeric = Number(value)
  return Number.isFinite(numeric) ? valueFormatter.format(numeric) : value
}

function sortRank(item: NormalizedDonutItem) {
  const normalizedKey = normalizeLabel(item.sortKey)
  const normalizedLabel = normalizeLabel(item.label)
  return stableLabelOrder.get(normalizedKey) ?? stableLabelOrder.get(normalizedLabel) ?? 1000
}

function resolveColor(item: NormalizedDonutItem) {
  const normalizedKey = normalizeLabel(item.colorKey)
  const normalizedLabel = normalizeLabel(item.label)
  return fixedColors.get(normalizedKey)
    ?? fixedColors.get(normalizedLabel)
    ?? item.color
    ?? colorFromLabel(normalizedKey || normalizedLabel)
}

function colorFromLabel(label: string) {
  let hash = 0
  for (const char of label) {
    hash = (hash * 31 + char.charCodeAt(0)) >>> 0
  }
  // 未登记的新资产也用确定性 hash 取色，刷新或接口顺序变化不会改变颜色。
  return fallbackPalette[hash % fallbackPalette.length]
}

function normalizeLabel(label: string) {
  return label.trim().toUpperCase()
}

function svgNumber(value: number) {
  // SVG dash 值保留有限小数，减少刷新时由浮点尾差造成的属性抖动。
  return Number(value.toFixed(4)).toString()
}
