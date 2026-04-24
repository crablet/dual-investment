<script setup lang="ts">
// 总览视图。
// 组合账户、收益、待办、事件日历和快捷候选入口；具体数据都来自 Pinia 快照派生。
import { computed } from 'vue'
import AccountBalanceList from '@/components/common/AccountBalanceList.vue'
import CandidateTable from '@/components/common/CandidateTable.vue'
import DonutChart from '@/components/common/DonutChart.vue'
import EventCalendarBoard from '@/components/common/EventCalendarBoard.vue'
import MetricCard from '@/components/common/MetricCard.vue'
import MiniLineChart from '@/components/common/MiniLineChart.vue'
import Panel from '@/components/common/Panel.vue'
import { useAppStore } from '@/stores/app'
import type { AppViewId } from '@/stores/app'
import type { StrategyCandidate } from '@/types/domain'

interface PendingTask {
  id: string
  tone: string
  text: string
  view: AppViewId
  visible: boolean
}

const store = useAppStore()

const activeStrategies = computed(() => store.orders.slice(0, 4))
const pnlCenter = computed(() => Number(store.pnl?.cumulative ?? 0).toLocaleString('zh-CN'))
const expiringOrdersCount = computed(() => {
  const today = new Date()
  const threshold = new Date(today.getTime() + 7 * 86400_000)
  return store.orders.filter((order) => {
    const expiry = new Date(order.expiryAt)
    return expiry >= today && expiry <= threshold
  }).length
})
// 待办只从当前快照派生，不新增独立任务存储，避免和订单/候选状态漂移。
const pendingTasks = computed<PendingTask[]>(() => {
  const tasks: PendingTask[] = [
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
const autoStatusItems = computed(() => [
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

function placeCandidate(candidate: StrategyCandidate) {
  // 总览快捷下单同样必须显式确认，确认后才把 `confirmLive` 传给 Rust。
  if (!window.confirm('确认发送 Binance 期权订单？')) {
    return
  }
  void store.placeStrategy({
    candidateId: candidate.id,
    symbol: candidate.symbol,
    optionSymbol: candidate.optionSymbol,
    direction: candidate.direction,
    expiry: candidate.expiry,
    expiryAt: candidate.expiryAt,
    strike: candidate.strike,
    quantity: candidate.quantity,
    limitPrice: candidate.limitPrice,
    confirmLive: true,
  })
}
</script>

<template>
  <div class="view-grid">
    <MetricCard v-for="metric in store.metrics" :key="metric.id" :metric="metric" />

    <Panel title="账户概览" class="span-3">
      <AccountBalanceList :balances="store.balances" />
    </Panel>

    <Panel title="收益趋势" class="span-5">
      <MiniLineChart v-if="store.pnl" :points="store.pnl.points" />
    </Panel>

    <Panel title="收益构成" class="span-4">
      <DonutChart v-if="store.pnl" :items="store.pnlBreakdown" :center="pnlCenter" />
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
            <strong :class="['auto-status-value', `tone-${item.tone}`]">{{ item.value }}</strong>
          </p>
        </div>
      </div>
    </Panel>

    <Panel title="事件日历看板" class="span-12">
      <EventCalendarBoard />
    </Panel>

    <Panel title="快捷机会" class="span-12">
      <CandidateTable :candidates="store.candidates.slice(0, 3)" @order="placeCandidate" />
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
