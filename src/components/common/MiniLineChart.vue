<script setup lang="ts">
import { computed } from 'vue'
import type { ChartPoint } from '@/types/domain'

const props = defineProps<{
  points: ChartPoint[]
}>()

const polyline = computed(() => {
  const values = props.points.map(point => Number(point.value))
  const min = Math.min(...values)
  const max = Math.max(...values)
  const span = max - min || 1
  return values
    .map((value, index) => {
      const x = (index / Math.max(values.length - 1, 1)) * 100
      const y = 100 - ((value - min) / span) * 84 - 8
      return `${x},${y}`
    })
    .join(' ')
})
</script>

<template>
  <div class="mini-chart">
    <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <linearGradient id="line-fill" x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stop-color="#f5b625" stop-opacity="0.35" />
          <stop offset="100%" stop-color="#f5b625" stop-opacity="0" />
        </linearGradient>
      </defs>
      <polyline :points="`0,100 ${polyline} 100,100`" fill="url(#line-fill)" stroke="none" />
      <polyline :points="polyline" fill="none" stroke="#f5b625" stroke-width="2.4" stroke-linecap="round" />
    </svg>
    <div class="chart-axis">
      <span v-for="point in points" :key="point.label">{{ point.label }}</span>
    </div>
  </div>
</template>
