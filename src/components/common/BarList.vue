<!-- 条形图列表。数值、格式和宽度由调用方提供。 -->
<script setup lang="ts">
import type { BarListItem } from '@/types/view'

defineProps<{
  items: BarListItem[]
}>()
</script>

<template>
  <div class="bar-list">
    <div v-for="item in items" :key="item.label">
      <span>{{ item.label }}</span>
      <strong :style="{ width: item.width }">{{ item.value }}</strong>
    </div>
  </div>
</template>
