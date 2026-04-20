<script setup lang="ts">
import { Activity, Coins, Shield, Wallet } from '@lucide/vue'
import { NIcon } from 'naive-ui'
import type { MetricCard } from '@/types/domain'

defineProps<{
  metric: MetricCard
}>()

const icons = {
  wallet: Wallet,
  shield: Shield,
  coins: Coins,
  activity: Activity,
}
</script>

<template>
  <article class="metric-card" :class="`tone-${metric.tone}`">
    <NIcon class="metric-icon" :component="icons[metric.icon as keyof typeof icons] ?? Wallet" />
    <div>
      <p class="metric-title">{{ metric.title }}</p>
      <strong>{{ metric.value }}</strong>
    </div>
  </article>
</template>
