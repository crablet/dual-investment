<!--
收益分析视图。

使用订单和收益快照做展示维度拆分，不在前端重算真实 PnL 事实。
-->
<script setup lang="ts">
import { computed } from 'vue'
import BarList from '@/components/common/BarList.vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import { relativeBarWidth } from '@/utils/bars'
import { localDateKey } from '@/utils/date'
import { decimalStringToDisplayNumber } from '@/utils/decimal'

const store = useAppStore()
const { directionText, money, percent } = useFormat()

const strategyPerformance = computed(() => {
  // 金额已经由 Rust 按方向汇总；number 只参与图形宽度计算。
  const rows = [
    { label: directionText('sellPut'), value: store.pnl?.sellPutPremium ?? '0' },
    { label: directionText('sellCall'), value: store.pnl?.sellCallPremium ?? '0' },
  ]
  const max = Math.max(...rows.map(row => decimalStringToDisplayNumber(row.value)), 1)
  return rows.map(row => ({
    label: row.label,
    value: money(row.value),
    width: relativeBarWidth(decimalStringToDisplayNumber(row.value), max),
  }))
})
</script>

<template>
  <div class="view-grid">
    <Panel title="收益汇总" class="span-6">
      <div v-if="store.pnl" class="summary-cards pnl-summary-cards">
        <article><span>累计收益</span><strong>{{ money(store.pnl.cumulative) }}</strong></article>
        <article><span>累计权利金</span><strong>{{ money(store.pnl.optionPremium) }}</strong></article>
        <article><span>补齐 BTC 浮动盈亏</span><strong>{{ money(store.pnl.floatingSpot) }}</strong></article>
      </div>
    </Panel>

    <Panel title="策略绩效对比" class="span-6">
      <BarList :items="strategyPerformance" />
    </Panel>

    <Panel title="收益明细" class="span-12">
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>日期</th>
              <th>策略</th>
              <th>收益金额</th>
              <th>年化</th>
              <th>备注</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="order in store.orders" :key="order.id">
              <td>{{ localDateKey(order.createdAt) }}</td>
              <td>{{ directionText(order.direction) }}</td>
              <td class="positive">{{ money(order.premium) }}</td>
              <td>{{ percent(order.annualized) }}</td>
              <td>{{ order.optionSymbol }} 收取权利金</td>
            </tr>
          </tbody>
        </table>
        <p v-if="store.orders.length === 0" class="empty-state">暂无收益明细</p>
      </div>
    </Panel>
  </div>
</template>
