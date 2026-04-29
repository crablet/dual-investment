<!--
轻量条形图列表。

只复用现有 .bar-list DOM 结构和样式；业务值、格式化文本和宽度都由调用方传入。
-->
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
