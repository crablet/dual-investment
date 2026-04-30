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
import { decimalStringToDisplayNumber } from '@/utils/decimal'

const store = useAppStore()
const { directionText, money, percent } = useFormat()

const strategyPerformance = computed(() => {
  // 条形图只比较当前前端订单快照中的权利金展示宽度，不替代 Rust 的收益报表事实。
  const rows = [
    { label: directionText('sellPut'), value: totalPremiumByDirection('sellPut') },
    { label: directionText('sellCall'), value: totalPremiumByDirection('sellCall') },
  ]
  const max = Math.max(...rows.map(row => row.value), 1)
  return rows.map(row => ({
    label: row.label,
    value: money(String(row.value)),
    width: relativeBarWidth(row.value, max),
  }))
})

/**
 * 按策略方向汇总权利金，用于图形展示。
 *
 * 这里转成展示 number 只影响 UI 宽度和标签格式；真实金额仍以后端 Decimal 字符串为准。
 */
function totalPremiumByDirection(direction: 'sellPut' | 'sellCall') {
  return store.orders
    .filter(order => order.direction === direction)
    .reduce((sum, order) => sum + decimalStringToDisplayNumber(order.premium), 0)
}
</script>

<template>
  <div class="view-grid">
    <Panel title="月度复盘" class="span-6">
      <div v-if="store.pnl" class="summary-cards monthly-summary-cards">
        <article><span>本月收益</span><strong>{{ money(store.pnl.cumulative) }}</strong></article>
        <article><span>本月权利金</span><strong>{{ money(store.pnl.optionPremium) }}</strong></article>
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
              <td>{{ order.createdAt.slice(0, 10) }}</td>
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
