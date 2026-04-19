<script setup lang="ts">
import { computed } from 'vue'
import DonutChart from '@/components/common/DonutChart.vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const { money } = useFormat()

const sellPutMargin = computed(() =>
  store.orders
    .filter(order => order.direction === 'sellPut')
    .reduce((sum, order) => sum + Number(order.strike) * Number(order.quantity), 0),
)
const sellCallQuantity = computed(() =>
  store.orders
    .filter(order => order.direction === 'sellCall')
    .reduce((sum, order) => sum + Number(order.quantity), 0),
)
const balanceBreakdown = computed(() =>
  store.balances.map((balance, index) => ({
    label: balance.asset,
    value: balance.usdtValue,
    color: ['#f5b625', '#5f8ff5', '#52c66f'][index % 3],
  })),
)
</script>

<template>
  <div class="view-grid">
    <Panel title="现货持仓" class="span-4">
      <div class="account-list large">
        <p v-if="store.balances.length === 0" class="empty-state">暂无实盘持仓</p>
        <div v-for="balance in store.balances" :key="balance.asset" class="account-row">
          <span>{{ balance.asset }}</span>
          <strong>{{ balance.free }}</strong>
          <em>{{ money(balance.usdtValue) }}</em>
        </div>
      </div>
    </Panel>

    <Panel title="义务仓概览" class="span-4">
      <div class="obligation-list">
        <article>
          <strong>卖出看跌义务仓</strong>
          <span>{{ store.orders.filter(order => order.direction === 'sellPut').length }} 个</span>
          <em>保证金占用 {{ money(String(sellPutMargin)) }}</em>
        </article>
        <article>
          <strong>卖出看涨义务仓</strong>
          <span>{{ store.orders.filter(order => order.direction === 'sellCall').length }} 个</span>
          <em>BTC 占用 {{ sellCallQuantity.toFixed(4) }}</em>
        </article>
      </div>
    </Panel>

    <Panel title="资金与保证金" class="span-4">
      <DonutChart
        :center="Number(store.risk?.marginUsage ?? 0).toFixed(1)"
        :items="balanceBreakdown"
      />
    </Panel>

    <Panel title="持仓明细" class="span-8">
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>标的</th>
              <th>持仓类型</th>
              <th>数量</th>
              <th>均价/执行价</th>
              <th>到期日</th>
              <th>浮动盈亏</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="order in store.orders" :key="order.id">
              <td>BTC</td>
              <td>{{ order.direction === 'sellPut' ? '卖出看跌（义务）' : '卖出看涨（义务）' }}</td>
              <td>{{ order.quantity }} BTC</td>
              <td>{{ money(order.strike) }}</td>
              <td>{{ order.expiry }}</td>
              <td class="positive">{{ money(order.estimatedPnl) }}</td>
              <td>正常运行</td>
            </tr>
            <tr v-for="balance in store.balances" :key="balance.asset">
              <td>{{ balance.asset }}</td>
              <td>现货</td>
              <td>{{ balance.free }}</td>
              <td>--</td>
              <td>--</td>
              <td>--</td>
              <td>持有中</td>
            </tr>
          </tbody>
        </table>
        <p v-if="store.orders.length === 0 && store.balances.length === 0" class="empty-state">暂无实盘持仓明细</p>
      </div>
    </Panel>

    <Panel title="到期日历" class="span-4">
      <div class="calendar-list">
        <p v-if="store.orders.length === 0" class="empty-state">暂无到期订单</p>
        <article v-for="order in store.orders" :key="order.id">
          <strong>{{ order.expiry }}</strong>
          <span>{{ order.direction === 'sellPut' ? '卖出看跌' : '卖出看涨' }}</span>
          <em>{{ order.quantity }} BTC</em>
        </article>
      </div>
    </Panel>
  </div>
</template>
