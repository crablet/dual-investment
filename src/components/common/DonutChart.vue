<!--
收益/资产构成环形图。

输入项是展示模型；组件负责稳定排序、稳定取色和交互态，不负责业务归因。
-->
<script setup lang="ts">
import type { PnlBreakdownItem } from '@/types/view'
import { computed, shallowRef } from 'vue'
import {
  absoluteDonutTotal,
  buildDonutSegments,
  centerDisplayForSegment,
  normalizeDonutItems,
} from '@/utils/donut'

const props = defineProps<{
  items: PnlBreakdownItem[]
  totalValue: string | null
  centerLabel: string
  emptyText: string
}>()

const hoverKey = shallowRef<string>()
const sortedItems = computed(() => normalizeDonutItems(props.items))

const absoluteTotal = computed(() => {
  return absoluteDonutTotal(sortedItems.value)
})

const hasItems = computed(() => absoluteTotal.value > 0)

const segments = computed(() => buildDonutSegments(sortedItems.value))

const activeSegment = computed(() =>
  segments.value.find(item => item.key === hoverKey.value),
)

const activeKey = computed(() => activeSegment.value?.key)

const centerDisplay = computed(() => {
  if (props.totalValue === null) {
    return { label: props.centerLabel, value: '--', meta: 'USDT', negative: false }
  }
  return centerDisplayForSegment(activeSegment.value, props.centerLabel, props.totalValue)
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
