<!--
总览指标卡。

接收前端派生好的展示模型，组件只处理图标映射和视觉 tone。
-->
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
