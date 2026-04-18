<script setup lang="ts">
import DonutChart from '@/components/common/DonutChart.vue'
import MiniLineChart from '@/components/common/MiniLineChart.vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const { money, percent } = useFormat()
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
        <div>
          <span>卖 Put 抄底</span>
          <strong style="width: 88%">21,486.35</strong>
        </div>
        <div>
          <span>卖 Call 止盈</span>
          <strong style="width: 64%">15,632.12</strong>
        </div>
        <div>
          <span>自动定投</span>
          <strong style="width: 36%">8,912.45</strong>
        </div>
      </div>
    </Panel>

    <Panel title="月度复盘" class="span-12">
      <div v-if="store.pnl" class="summary-cards">
        <article><span>本月收益</span><strong>{{ money(store.pnl.cumulative) }}</strong><em>较上月 +23.42%</em></article>
        <article><span>本月权利金</span><strong>{{ money(store.pnl.optionPremium) }}</strong><em>占比 25.28%</em></article>
        <article><span>本月已实现盈亏</span><strong>{{ money(store.pnl.realizedSpot) }}</strong><em>占比 43.96%</em></article>
        <article><span>本月浮动盈亏</span><strong>{{ money(store.pnl.floatingSpot) }}</strong><em>占比 21.88%</em></article>
        <article><span>手续费支出</span><strong>{{ money(store.pnl.fees) }}</strong><em>占比 -3.41%</em></article>
        <article><span>最大回撤</span><strong>-6.42%</strong><em>发生于 06-02</em></article>
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
              <td class="positive">{{ percent(order.direction === 'sellPut' ? '18.56' : '15.02') }}</td>
              <td>{{ order.optionSymbol }} 收取权利金</td>
            </tr>
          </tbody>
        </table>
      </div>
    </Panel>
  </div>
</template>
