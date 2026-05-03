<!--
总览视图。

组合账户、收益、活跃策略、自动状态和事件日历；具体数据都来自 Pinia 快照派生。
-->
<script setup lang="ts">
import { computed } from 'vue'
import AccountBalanceList from '@/components/common/AccountBalanceList.vue'
import DonutChart from '@/components/common/DonutChart.vue'
import EventCalendarBoard from '@/components/common/EventCalendarBoard.vue'
import MetricCard from '@/components/common/MetricCard.vue'
import Panel from '@/components/common/Panel.vue'
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

const autoStatusItems = computed(() => [
  // 这里是导航状态摘要，不是自动策略执行条件；真正门禁仍在 Rust 后台 tick 中判断。
  {
    label: '后台同步',
    value: runtimeHealthLabel.value,
    tone: store.health?.status === 'healthy' ? 'active' : store.health?.status === 'idle' ? 'neutral' : 'paused',
  },
  {
    label: '自动扫描',
    value: store.autoRule?.enabled ? '已启用' : '已暂停',
    tone: store.autoRule?.enabled ? 'active' : 'paused',
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
      <div class="order-strip">
        <article v-for="order in activeStrategies" :key="order.id">
          <strong>{{ order.id }}</strong>
          <span>{{ order.optionSymbol }}</span>
          <em>{{ order.settlementAction }}</em>
        </article>
      </div>
    </Panel>

    <Panel title="自动定投状态" class="span-6 auto-status-panel">
      <div class="auto-state">
        <div class="auto-scan-card">
          <span>扫描计划</span>
          <strong>{{ store.autoRule?.schedule ?? '未设置' }}</strong>
          <em>{{ store.settings?.connected ? '已连接' : '等待连接' }}</em>
        </div>
        <div class="auto-status-list">
          <p v-for="item in autoStatusItems" :key="item.label" class="auto-status-item">
            <span>{{ item.label }}</span>
            <strong class="auto-status-value" :class="[`tone-${item.tone}`]">{{ item.value }}</strong>
          </p>
        </div>
      </div>
    </Panel>

    <Panel title="事件日历看板" class="span-12">
      <EventCalendarBoard />
    </Panel>
  </div>
</template>
