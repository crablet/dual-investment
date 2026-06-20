// 环形图的排序、配色与 SVG 占比计算。

import type { PnlBreakdownItem } from '@/types/view'
import { formatLocale, t } from '@/i18n'
import {
  decimalStringToDisplayNumber,
  isNegativeDecimalString,
  isZeroDecimalString,
} from '@/utils/decimal'

/** 环形图内部归一化条目，补齐渲染、排序和颜色需要的稳定键。 */
export interface NormalizedDonutItem extends PnlBreakdownItem {
  /** Vue 渲染使用的唯一 key；同业务 key 重复时会追加序号。 */
  key: string
  /** 颜色解析使用的稳定业务 key，避免重复项改变同类颜色。 */
  colorKey: string
  /** 排序使用的稳定业务 key，和渲染 key 分离以保留业务顺序。 */
  sortKey: string
  /** 原始输入顺序，用于排序字段完全相同时保持稳定。 */
  sourceIndex: number
}

/** SVG 扇区展示模型，所有字段都只服务图形和辅助阅读文案。 */
export interface DonutSegment extends NormalizedDonutItem {
  color: string
  percentLabel: string
  valueLabel: string
  dasharray: string
  dashoffset: string
  negative: boolean
  ariaLabel: string
}

/** 环形图中心文案，随 hover/focus 在总览和单扇区之间切换。 */
export interface DonutCenterDisplay {
  label: string
  value: string
  meta: string
  negative: boolean
}

// 业务上有稳定含义的标签固定顺序，避免接口返回顺序变化导致扇区位置跳动。
const stableLabelOrder = new Map<string, number>([
  ['USDT', 100],
  ['USDC', 110],
  ['BTC', 120],
  ['BNB', 130],
  ['BNSOL', 140],
  ['TON', 150],
  ['WBETH', 160],
])

const fixedColors = new Map<string, string>([
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

function collator() {
  return new Intl.Collator(formatLocale(), {
    numeric: true,
    sensitivity: 'base',
  })
}

function valueFormatter() {
  return new Intl.NumberFormat(formatLocale(), {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })
}

function percentFormatter() {
  return new Intl.NumberFormat(formatLocale(), {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  })
}

/** 归一化环形图输入，过滤零值并按业务稳定顺序排序。 */
export function normalizeDonutItems(items: PnlBreakdownItem[]): NormalizedDonutItem[] {
  const keyCounts = new Map<string, number>()

  return items
    .map((item, index): NormalizedDonutItem => {
      const baseKey = item.key
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
      const keyCompare = collator().compare(a.key, b.key)
      return keyCompare === 0 ? a.sourceIndex - b.sourceIndex : keyCompare
    })
}

/** 负值按绝对值计算占比，避免抵消其他扇区。 */
export function absoluteDonutTotal(items: NormalizedDonutItem[]) {
  return items.reduce((sum, item) => sum + Math.abs(decimalStringToDisplayNumber(item.value)), 0)
}

/** 把归一化条目转换成 SVG circle 需要的 dash 参数和可访问文案。 */
export function buildDonutSegments(items: NormalizedDonutItem[]): DonutSegment[] {
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
      percentLabel: `${percentFormatter().format(percent)}%`,
      valueLabel: formatDonutValue(item.value),
      dasharray: `${svgNumber(length)} ${svgNumber(100 - length)}`,
      dashoffset: svgNumber(offset),
      negative: isNegativeDecimalString(item.value),
      ariaLabel: t('ui.usdt', { p0: item.label, p1: formatDonutValue(item.value), p2: percentFormatter().format(percent) }),
    }
    offset -= length
    return segment
  })
}

/** 根据 hover/focus 状态决定中心区域展示总览还是单个扇区。 */
export function centerDisplayForSegment(
  active: DonutSegment | undefined,
  centerLabel: string,
  centerValue: string,
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
    meta: 'USDT',
    negative: isNegativeDecimalString(centerValue),
  }
}

/** 无法解析的数值保留原文，便于排查数据问题。 */
export function formatDonutValue(value: string) {
  const numeric = Number(value)
  return Number.isFinite(numeric) ? valueFormatter().format(numeric) : value
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
