<script setup lang="ts">
import { computed } from 'vue'
import type { PnlBreakdown } from '@/types/domain'

const props = defineProps<{
  items: PnlBreakdown[]
  center: string
}>()

const total = computed(() =>
  props.items.reduce((sum, item) => sum + Math.abs(Number(item.value)), 0) || 1,
)

const segments = computed(() => {
  let offset = 25
  return props.items.map((item) => {
    const length = (Math.abs(Number(item.value)) / total.value) * 100
    const segment = {
      ...item,
      dasharray: `${length} ${100 - length}`,
      dashoffset: `${offset}`,
    }
    offset -= length
    return segment
  })
})
</script>

<template>
  <div class="donut-wrap">
    <svg class="donut" viewBox="0 0 42 42" aria-hidden="true">
      <circle class="donut-bg" cx="21" cy="21" r="15.915" />
      <circle
        v-for="item in segments"
        :key="item.label"
        class="donut-segment"
        cx="21"
        cy="21"
        r="15.915"
        :stroke="item.color"
        :stroke-dasharray="item.dasharray"
        :stroke-dashoffset="item.dashoffset"
      />
      <text x="21" y="20" text-anchor="middle">{{ center }}</text>
      <text x="21" y="25" text-anchor="middle">USDT</text>
    </svg>
    <div class="donut-legend">
      <div v-for="item in items" :key="item.label" class="legend-row">
        <span :style="{ background: item.color }" />
        <p>{{ item.label }}</p>
        <strong>{{ Number(item.value).toLocaleString('zh-CN') }}</strong>
      </div>
    </div>
  </div>
</template>
