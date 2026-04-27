<!--
收益/资产构成环形图。

输入项是展示模型；组件负责稳定排序、稳定取色和交互态，不负责业务归因。
-->
<script setup lang="ts">
import type { PnlBreakdownItem } from '@/types/view'
import { computed, shallowRef } from 'vue'
import { isNegativeDecimalString, isZeroDecimalString, sumDecimalStrings } from '@/utils/decimal'

interface NormalizedDonutItem extends PnlBreakdownItem {
  key: string
  colorKey: string
  sortKey: string
  sourceIndex: number
}

interface DonutSegment extends NormalizedDonutItem {
  color: string
  percentLabel: string
  valueLabel: string
  dasharray: string
  dashoffset: string
  negative: boolean
  ariaLabel: string
}

const props = withDefaults(defineProps<{
  items: PnlBreakdownItem[]
  center?: string
  centerValue?: string
  centerLabel?: string
  unit?: string
  emptyText?: string
}>(), {
  center: undefined,
  centerValue: undefined,
  centerLabel: '合计',
  unit: 'USDT',
  emptyText: '暂无构成数据',
})

const hoverKey = shallowRef<string>()

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

const sortedItems = computed(() => {
  const keyCounts = new Map<string, number>()

  return props.items
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
})

const absoluteTotal = computed(() => {
  // 饼图用绝对值计算占比，负收益仍以颜色/文案标识，不让负数抵消总占比。
  return sortedItems.value.reduce((sum, item) => sum + Math.abs(decimalToNumber(item.value)), 0)
})

const hasItems = computed(() => absoluteTotal.value > 0)

const segments = computed<DonutSegment[]>(() => {
  let offset = 25
  if (absoluteTotal.value <= 0) {
    return []
  }

  return sortedItems.value.map((item) => {
    const numericValue = decimalToNumber(item.value)
    const length = (Math.abs(numericValue) / absoluteTotal.value) * 100
    const percent = (Math.abs(numericValue) / absoluteTotal.value) * 100
    const segment = {
      ...item,
      color: resolveColor(item),
      percentLabel: `${percentFormatter.format(percent)}%`,
      valueLabel: formatValue(item.value),
      dasharray: `${svgNumber(length)} ${svgNumber(100 - length)}`,
      dashoffset: svgNumber(offset),
      negative: isNegativeDecimalString(item.value),
      ariaLabel: `${item.label}，${formatValue(item.value)} ${props.unit}，占比 ${percentFormatter.format(percent)}%`,
    }
    offset -= length
    return segment
  })
})

const activeSegment = computed(() =>
  segments.value.find(item => item.key === hoverKey.value),
)

const activeKey = computed(() => activeSegment.value?.key)

const centerValue = computed(() =>
  props.centerValue
  ?? props.center
  ?? sumDecimalStrings(sortedItems.value.map(item => item.value)),
)

const centerDisplay = computed(() => {
  const active = activeSegment.value
  if (active) {
    return {
      label: active.label,
      value: active.valueLabel,
      meta: active.percentLabel,
      negative: active.negative,
    }
  }
  return {
    label: props.centerLabel,
    value: formatValue(centerValue.value),
    meta: props.unit,
    negative: isNegativeDecimalString(centerValue.value),
  }
})

function setHover(key: string) {
  // 只保留临时 hover/focus 状态；不支持点击后永久选中，鼠标离开即可恢复总览。
  hoverKey.value = key
}

function clearHover() {
  hoverKey.value = undefined
}

function isActive(key: string) {
  return activeKey.value === key
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

function decimalToNumber(value: string) {
  const numeric = Number(value)
  return Number.isFinite(numeric) ? numeric : 0
}

function formatValue(value: string) {
  const numeric = Number(value)
  return Number.isFinite(numeric) ? valueFormatter.format(numeric) : value
}

function svgNumber(value: number) {
  // SVG dash 值保留有限小数，减少刷新时由浮点尾差造成的属性抖动。
  return Number(value.toFixed(4)).toString()
}
</script>

<template>
  <div class="donut-wrap">
    <svg class="donut" viewBox="0 0 42 42" role="group" aria-label="构成环形图">
      <circle class="donut-bg" cx="21" cy="21" r="15.915" />
      <circle
        v-for="item in segments"
        :key="item.key"
        class="donut-segment"
        :class="{ active: isActive(item.key), muted: Boolean(activeKey) && !isActive(item.key) }"
        cx="21"
        cy="21"
        r="15.915"
        role="button"
        tabindex="0"
        :aria-label="item.ariaLabel"
        :stroke="item.color"
        :stroke-dasharray="item.dasharray"
        :stroke-dashoffset="item.dashoffset"
        @focus="setHover(item.key)"
        @blur="clearHover"
        @pointerenter="setHover(item.key)"
        @pointerleave="clearHover"
      />
      <text class="donut-center-label" x="21" y="16.6" text-anchor="middle">
        {{ centerDisplay.label }}
      </text>
      <text
        class="donut-center-value"
        :class="{ negative: centerDisplay.negative }"
        x="21"
        y="22.1"
        text-anchor="middle"
      >
        {{ centerDisplay.value }}
      </text>
      <text class="donut-center-meta" x="21" y="27.2" text-anchor="middle">
        {{ centerDisplay.meta }}
      </text>
    </svg>
    <div class="donut-legend">
      <p v-if="!hasItems" class="empty-state">{{ props.emptyText }}</p>
      <button
        v-for="item in segments"
        :key="item.key"
        type="button"
        class="legend-row"
        :class="{ active: isActive(item.key) }"
        @focus="setHover(item.key)"
        @blur="clearHover"
        @pointerenter="setHover(item.key)"
        @pointerleave="clearHover"
      >
        <span :style="{ backgroundColor: item.color }" />
        <p>{{ item.label }}</p>
        <strong :class="{ negative: item.negative }">{{ item.valueLabel }}</strong>
      </button>
    </div>
  </div>
</template>

<style scoped>
.donut-wrap {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  align-items: center;
  gap: 18px;
}

.donut {
  width: 164px;
  height: 164px;
  justify-self: center;
  overflow: visible;
}

.donut-bg {
  fill: transparent;
  stroke: rgba(255, 255, 255, 0.08);
  stroke-width: 6;
}

.donut-segment {
  fill: transparent;
  stroke-width: 6;
  cursor: pointer;
  outline: none;
  transition:
    opacity 160ms ease,
    stroke-width 160ms ease,
    filter 160ms ease;
}

.donut-segment.active,
.donut-segment:focus-visible {
  stroke-width: 7.4;
  filter: drop-shadow(0 0 5px rgba(255, 255, 255, 0.24));
}

.donut-segment.muted {
  opacity: 0.36;
}

.donut text {
  pointer-events: none;
  font-variant-numeric: tabular-nums;
}

.donut-center-label,
.donut-center-meta {
  fill: var(--muted);
  font-size: 3px;
}

.donut-center-value {
  fill: var(--text);
  font-size: 4px;
  font-weight: 700;
}

.donut-center-value.negative {
  fill: var(--red);
}

.donut-legend {
  display: grid;
  gap: 8px;
  min-width: 0;
}

.legend-row {
  display: grid;
  grid-template-columns: 10px minmax(76px, 1fr) auto;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-width: 0;
  min-height: 28px;
  padding: 4px 6px;
  border: 1px solid transparent;
  border-radius: 6px;
  color: inherit;
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.legend-row:hover,
.legend-row.active,
.legend-row:focus-visible {
  border-color: rgba(255, 255, 255, 0.12);
  background: rgba(255, 255, 255, 0.04);
  outline: none;
}

.legend-row span {
  width: 10px;
  height: 10px;
  border-radius: 50%;
}

.legend-row p {
  margin: 0;
  overflow: hidden;
  color: var(--muted);
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.legend-row strong {
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}

.legend-row strong.negative {
  color: var(--red);
}
</style>
