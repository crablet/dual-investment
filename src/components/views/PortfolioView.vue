<script setup lang="ts">
import DonutChart from '@/components/common/DonutChart.vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const { money } = useFormat()
</script>

<template>
  <div class="view-grid">
    <Panel title="现货持仓" class="span-4">
      <div class="account-list large">
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
          <em>保证金占用 {{ money('5248.32') }}</em>
        </article>
        <article>
          <strong>卖出看涨义务仓</strong>
          <span>{{ store.orders.filter(order => order.direction === 'sellCall').length }} 个</span>
          <em>BTC 占用 0.01</em>
        </article>
      </div>
    </Panel>

    <Panel title="资金与保证金" class="span-4">
      <DonutChart
        :center="Number(store.risk?.marginUsage ?? 0).toFixed(1)"
        :items="[
          { label: 'USDT', value: '8906.00', color: '#f5b625' },
          { label: 'BTC', value: '3050.18', color: '#6556e8' },
          { label: '其他', value: '400.00', color: '#39b7c9' },
        ]"
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
              <td>{{ balance.asset === 'BTC' ? money('70086.72') : money('1') }}</td>
              <td>--</td>
              <td :class="balance.asset === 'BTC' ? 'positive' : ''">{{ balance.asset === 'BTC' ? money('1245.73') : '--' }}</td>
              <td>持有中</td>
            </tr>
          </tbody>
        </table>
      </div>
    </Panel>

    <Panel title="到期日历" class="span-4">
      <div class="calendar-list">
        <article v-for="order in store.orders" :key="order.id">
          <strong>{{ order.expiry }}</strong>
          <span>{{ order.direction === 'sellPut' ? '卖出看跌' : '卖出看涨' }}</span>
          <em>{{ order.quantity }} BTC</em>
        </article>
      </div>
    </Panel>
  </div>
</template>
