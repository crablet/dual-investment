<script setup lang="ts">
import type { AppViewId } from '@/stores/app'
// 总览视图。
// 组合账户、收益、待办、事件日历和活跃策略；具体数据都来自 Pinia 快照派生。
import { computed } from 'vue'
import AccountBalanceList from '@/components/common/AccountBalanceList.vue'
import DonutChart from '@/components/common/DonutChart.vue'
import EventCalendarBoard from '@/components/common/EventCalendarBoard.vue'
import MetricCard from '@/components/common/MetricCard.vue'
import Panel from '@/components/common/Panel.vue'
import { useAppStore } from '@/stores/app'

interface PendingTask {
  id: string
  tone: string
  text: string
  view: AppViewId
  visible: boolean
}

const store = useAppStore()

const activeStrategies = computed(() => store.runningOrders.slice(0, 4))
const expiringOrdersCount = computed(() => {
  const today = new Date()
  const threshold = new Date(today.getTime() + 7 * 86400_000)
  return store.orders.filter((order) => {
    const expiry = new Date(order.expiryAt)
    return expiry >= today && expiry <= threshold
  }).length
})
const runtimeHealthAttention = computed(() => {
  const health = store.health
  return Boolean(
    health
    && (health.consecutiveFailures > 0 || health.skippedTicks > 0)
    && (health.status === 'degraded' || health.status === 'attention'),
  )
})
const runtimeHealthText = computed(() => {
  const health = store.health
  if (!health) {
    return '后台同步状态未知'
  }
  if (health.consecutiveFailures > 0) {
    return `后台同步连续失败 ${health.consecutiveFailures} 次`
  }
  return `后台同步已跳过 ${health.skippedTicks} 轮`
})
// 待办只从当前快照派生，不新增独立任务存储，避免和订单/候选状态漂移。
const pendingTasks = computed<PendingTask[]>(() => {
  const tasks: PendingTask[] = [
    {
      id: 'pending-recovery',
      tone: 'warning',
      text: `${store.pendingRecoveryCount} 条恢复记录待确认`,
      view: 'orders',
      visible: store.pendingRecoveryCount > 0,
    },
    {
      id: 'runtime-health',
      tone: 'warning',
      text: runtimeHealthText.value,
      view: 'risk',
      visible: runtimeHealthAttention.value,
    },
    {
      id: 'expiring-orders',
      tone: '',
      text: `${expiringOrdersCount.value} 个订单将在 7 天内到期`,
      view: 'orders',
      visible: expiringOrdersCount.value > 0,
    },
    {
      id: 'candidate-count',
      tone: 'blue',
      text: `当前有 ${store.candidates.length} 个候选机会`,
      view: 'dual',
      visible: store.candidates.length > 0,
    },
  ]
  return tasks.filter(item => item.visible)
})
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

    <Panel title="待处理事项" class="span-6">
      <div class="task-list">
        <p v-if="pendingTasks.length === 0" class="empty-state">暂无待处理事项</p>
        <div v-for="(task, index) in pendingTasks" :key="task.id" class="task-row">
          <span class="task-index" :class="task.tone">{{ index + 1 }}</span>
          {{ task.text }}
          <button type="button" @click="store.openView(task.view)">查看</button>
        </div>
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

    <Panel title="活跃策略" class="span-12">
      <div class="order-strip">
        <article v-for="order in activeStrategies" :key="order.id">
          <strong>{{ order.id }}</strong>
          <span>{{ order.optionSymbol }}</span>
          <em>{{ order.settlementAction }}</em>
        </article>
      </div>
    </Panel>
  </div>
</template>
