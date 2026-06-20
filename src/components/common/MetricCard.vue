<script setup lang="ts">
import type { MetricCardModel } from '@/types/view'
import { Coins, ShieldCheck, Wallet } from '@lucide/vue'

defineProps<{ metric: MetricCardModel }>()
const icons = { portfolio: Wallet, margin: ShieldCheck, premium: Coins }
</script>

<template>
  <article class="metric-card" :class="`metric-${metric.id}`">
    <p class="metric-title">
      <component :is="icons[metric.id]" :size="17" aria-hidden="true" />
      {{ metric.title }}
    </p>
    <div class="metric-amount">
      <strong>{{ metric.value }}</strong>
      <span>USDT</span>
    </div>
  </article>
</template>
