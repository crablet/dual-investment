<script setup lang="ts">
import { computed } from 'vue'
import DonutChart from '@/components/common/DonutChart.vue'
import MiniLineChart from '@/components/common/MiniLineChart.vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const { money, percent } = useFormat()

const strategyPerformance = computed(() => {
  const rows = [
    { label: '卖 Put 抄底', value: totalPremiumByDirection('sellPut') },
    { label: '卖 Call 止盈', value: totalPremiumByDirection('sellCall') },
  ]
  const max = Math.max(...rows.map(row => row.value), 1)
  return rows.map(row => ({
    ...row,
    width: `${Math.max((row.value / max) * 100, row.value > 0 ? 10 : 0)}%`,
  }))
})

function totalPremiumByDirection(direction: 'sellPut' | 'sellCall') {
  return store.orders
    .filter(order => order.direction === direction)
    .reduce((sum, order) => sum + Number(order.premium), 0)
}
</script>

<template>
  <div class="view-grid">
    <Panel title="收益趋势" class="span-5">
      <MiniLineChart v-if="store.pnl" :points="store.pnl.points" />
    </Panel>

    <Panel title="收益构成" class="span-4">
      <DonutChart
        v-if="store.pnl"
        :items="store.pnl.breakdown"
        :center="Number(store.pnl.cumulative).toLocaleString('zh-CN')"
      />
    </Panel>

    <Panel title="策略绩效对比" class="span-3">
      <div class="bar-list">
        <div v-for="row in strategyPerformance" :key="row.label">
          <span>{{ row.label }}</span>
          <strong :style="{ width: row.width }">{{ money(String(row.value)) }}</strong>
        </div>
      </div>
    </Panel>

    <Panel title="月度复盘" class="span-12">
      <div v-if="store.pnl" class="summary-cards">
        <article><span>本月收益</span><strong>{{ money(store.pnl.cumulative) }}</strong><em>实盘记录</em></article>
        <article><span>本月权利金</span><strong>{{ money(store.pnl.optionPremium) }}</strong><em>实盘记录</em></article>
        <article><span>本月已实现盈亏</span><strong>{{ money(store.pnl.realizedSpot) }}</strong><em>实盘记录</em></article>
        <article><span>本月浮动盈亏</span><strong>{{ money(store.pnl.floatingSpot) }}</strong><em>实盘记录</em></article>
        <article><span>手续费支出</span><strong>{{ money(store.pnl.fees) }}</strong><em>实盘记录</em></article>
        <article><span>最大回撤</span><strong>{{ percent(store.risk?.dailyDrawdown ?? '0') }}</strong><em>本地风险记录</em></article>
      </div>
    </Panel>

    <Panel title="收益明细" class="span-12">
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>日期</th>
              <th>策略</th>
              <th>收益类型</th>
              <th>收益金额</th>
              <th>年化</th>
              <th>备注</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="order in store.orders" :key="order.id">
              <td>{{ order.createdAt.slice(0, 10) }}</td>
              <td>{{ order.direction === 'sellPut' ? '卖 Put 抄底' : '卖 Call 止盈' }}</td>
              <td>权利金收益</td>
              <td class="positive">{{ money(order.premium) }}</td>
              <td>--</td>
              <td>{{ order.optionSymbol }} 收取权利金</td>
            </tr>
          </tbody>
        </table>
        <p v-if="store.orders.length === 0" class="empty-state">暂无实盘收益明细</p>
      </div>
    </Panel>
  </div>
</template>
