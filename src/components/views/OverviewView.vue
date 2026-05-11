<!--
总览视图。

组合账户、收益、活跃策略、运行状态和事件日历；具体数据都来自 Pinia 快照派生。
-->
<script setup lang="ts">
import { computed } from 'vue'
import AccountBalanceList from '@/components/common/AccountBalanceList.vue'
import DonutChart from '@/components/common/DonutChart.vue'
import EventCalendarBoard from '@/components/common/EventCalendarBoard.vue'
import MetricCard from '@/components/common/MetricCard.vue'
import Panel from '@/components/common/Panel.vue'
import ActiveStrategyList from '@/components/overview/ActiveStrategyList.vue'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

// 总览只展示最近几条活跃策略作为入口；完整订单事实仍在订单页和后端快照中。
const activeStrategies = computed(() => store.runningOrders.slice(0, 4))

const runtimeHealthLabel = computed(() => {
  const health = store.health
  if (!health || health.status === 'idle')
    return '等待连接'
  if (health.status === 'healthy')
    return '运行正常'
  if (health.status === 'degraded')
    return '需要关注'
  return '待处理'
})

const runtimeStatusItems = computed(() => [
  // 这里只展示后台维护状态；恢复、结算和资金动作仍以 Rust 快照和门禁为准。
  {
    label: '后台同步',
    value: runtimeHealthLabel.value,
    tone: store.health?.status === 'healthy' ? 'active' : store.health?.status === 'idle' ? 'neutral' : 'paused',
  },
  {
    label: '快速成交',
    value: store.settings?.quickExecution ? '已启用' : '未启用',
    tone: store.settings?.quickExecution ? 'active' : 'paused',
  },
  {
    label: '到期补齐',
    value: store.settings?.connected ? '后台监控' : '等待连接',
    tone: store.settings?.connected ? 'active' : 'neutral',
  },
])
</script>

<template>
  <div class="view-grid">
    <MetricCard v-for="metric in store.metrics" :key="metric.id" :metric="metric" />

    <Panel title="账户概览" class="span-6">
      <AccountBalanceList :balances="store.balances" />
    </Panel>

    <Panel title="收益构成" class="span-6">
      <DonutChart
        v-if="store.pnl"
        :items="store.pnlBreakdown"
        :center-value="store.pnl.cumulative"
        center-label="累计收益"
        empty-text="暂无收益构成数据"
      />
    </Panel>

    <Panel title="活跃策略" class="span-6">
      <ActiveStrategyList :orders="activeStrategies" />
    </Panel>

    <Panel title="运行状态" class="span-6 runtime-status-panel">
      <div class="runtime-status-list">
        <p v-for="item in runtimeStatusItems" :key="item.label" class="runtime-status-item">
          <span>{{ item.label }}</span>
          <strong class="runtime-status-value" :class="[`tone-${item.tone}`]">{{ item.value }}</strong>
        </p>
      </div>
    </Panel>

    <Panel title="事件日历看板" class="span-12">
      <EventCalendarBoard />
    </Panel>
  </div>
</template>
