<!--
持仓与现货视图。

展示期权义务仓和现货资产；所有资金事实来自后端快照，前端仅做展示聚合。
-->
<script setup lang="ts">
import type { BalanceRow } from '@/types/domain'
import type { PnlBreakdownItem } from '@/types/view'
import { computed } from 'vue'
import AccountBalanceList from '@/components/common/AccountBalanceList.vue'
import DonutChart from '@/components/common/DonutChart.vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import { localDateTimeLabel } from '@/utils/date'
import { sumDecimalStrings } from '@/utils/decimal'

const store = useAppStore()
const { decimal, money } = useFormat()

const obligationOrders = computed(() =>
  store.orders.filter(order => order.status === 'running' || order.status === 'partiallyFilled'),
)
// 这里的义务仓暴露额用于 UI 概览，真实保证金校验仍在 Rust 风控中完成。
const sellPutMargin = computed(() =>
  obligationOrders.value
    .filter(order => order.direction === 'sellPut')
    .reduce((sum, order) => sum + Number(order.strike) * Number(order.quantity), 0),
)
const sellCallQuantity = computed(() =>
  obligationOrders.value
    .filter(order => order.direction === 'sellCall')
    .reduce((sum, order) => sum + Number(order.quantity), 0),
)
const balanceBreakdown = computed<PnlBreakdownItem[]>(() => buildBalanceBreakdown(store.balances))
const balanceTotalValue = computed(() =>
  sumDecimalStrings(balanceBreakdown.value.map(item => item.value)),
)

function expiryLabel(order: { expiryAt: string }) {
  return localDateTimeLabel(order.expiryAt)
}

function buildBalanceBreakdown(balances: BalanceRow[]): PnlBreakdownItem[] {
  const grouped = new Map<string, string[]>()
  for (const balance of balances) {
    if (!balance.usdtValueKnown) {
      // 后端无法定价的资产不能参与比例图；账户概览仍会显示“暂无估值”。
      continue
    }
    const asset = balance.asset.trim().toUpperCase()
    const values = grouped.get(asset) ?? []
    values.push(balance.usdtValue)
    grouped.set(asset, values)
  }

  return [...grouped.entries()].map(([asset, values]) => ({
    key: asset,
    label: asset,
    value: sumDecimalStrings(values),
  }))
}
</script>

<template>
  <div class="view-grid">
    <Panel title="账户概览" class="span-4">
      <AccountBalanceList :balances="store.balances" class="large" empty-text="暂无持仓" />
    </Panel>

    <Panel title="义务仓概览" class="span-4">
      <div class="obligation-list">
        <article>
          <strong>卖出看跌义务仓</strong>
          <span>{{ obligationOrders.filter(order => order.direction === 'sellPut').length }} 个</span>
          <em>保证金占用 {{ money(String(sellPutMargin)) }}</em>
        </article>
        <article>
          <strong>卖出看涨义务仓</strong>
          <span>{{ obligationOrders.filter(order => order.direction === 'sellCall').length }} 个</span>
          <em>BTC 占用 {{ decimal(sellCallQuantity) }}</em>
        </article>
      </div>
    </Panel>

    <Panel title="资金与保证金" class="span-4">
      <DonutChart
        :items="balanceBreakdown"
        :center-value="balanceTotalValue"
        center-label="资产估值"
        empty-text="暂无可估值资产"
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
            <tr v-for="order in obligationOrders" :key="order.id">
              <td>BTC</td>
              <td>{{ order.direction === 'sellPut' ? '卖出看跌（义务）' : '卖出看涨（义务）' }}</td>
              <td>{{ decimal(order.quantity) }} BTC</td>
              <td>{{ money(order.strike) }}</td>
              <td>{{ expiryLabel(order) }}</td>
              <td class="positive">{{ money(order.estimatedPnl) }}</td>
              <td>{{ order.status === 'partiallyFilled' ? '部分成交' : '运行中' }}</td>
            </tr>
            <tr
              v-for="(balance, index) in store.balances"
              :key="`${balance.source}:${balance.asset}:${index}`"
            >
              <td>{{ balance.asset }}</td>
              <td>现货</td>
              <td>{{ decimal(balance.free) }}</td>
              <td>--</td>
              <td>--</td>
              <td>--</td>
              <td>持有中</td>
            </tr>
          </tbody>
        </table>
        <p v-if="store.orders.length === 0 && store.balances.length === 0" class="empty-state">暂无持仓明细</p>
      </div>
    </Panel>

    <Panel title="到期日历" class="span-4">
      <div class="calendar-list">
        <p v-if="store.orders.length === 0" class="empty-state">暂无到期订单</p>
        <article v-for="order in store.orders" :key="order.id">
          <strong>{{ expiryLabel(order) }}</strong>
          <span>{{ order.direction === 'sellPut' ? '卖出看跌' : '卖出看涨' }}</span>
          <em>{{ order.quantity }} BTC</em>
        </article>
      </div>
    </Panel>
  </div>
</template>
