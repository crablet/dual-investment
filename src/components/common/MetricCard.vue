<!-- 指标卡，按传入模型显示数值、图标和颜色。 -->
<script setup lang="ts">
import type { MetricCardModel } from '@/types/view'
import { Activity, Coins, Shield, Wallet } from '@lucide/vue'
import { NIcon } from 'naive-ui'

defineProps<{
  metric: MetricCardModel
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
    <NIcon class="metric-icon" :component="icons[metric.icon]" />
    <div>
      <p class="metric-title">{{ metric.title }}</p>
      <strong>{{ metric.value }}</strong>
    </div>
  </article>
</template>
