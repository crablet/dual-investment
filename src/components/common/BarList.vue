<script setup lang="ts">
import type { BarListItem } from '@/types/view'

defineProps<{ items: BarListItem[] }>()
</script>

<template>
  <div class="bar-list">
    <div v-for="item in items" :key="item.label" class="bar-row">
      <span>{{ item.label }}</span><strong>{{ item.value }}</strong>
      <div class="bar-track" aria-hidden="true"><i :style="{ width: item.width }" /></div>
    </div>
  </div>
</template>
