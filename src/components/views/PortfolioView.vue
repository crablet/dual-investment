<!--
持仓与现货视图。

展示期权义务仓和现货资产；金额与数量已在 Rust 汇总，前端组织列表和图表。
-->
<script setup lang="ts">
import type { PnlBreakdownItem } from '@/types/view'
import { NTooltip } from 'naive-ui'
import { computed, onMounted, onUnmounted, shallowRef } from 'vue'
import AccountBalanceList from '@/components/common/AccountBalanceList.vue'
import DonutChart from '@/components/common/DonutChart.vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import { presentAccountBalances } from '@/utils/balances'
import { localDateTimeLabel } from '@/utils/date'
import { decimalStringHasValue } from '@/utils/decimal'

const store = useAppStore()
const { decimal, money } = useFormat()
const currentTimeMs = shallowRef(Date.now())
let currentTimeTimer: number | undefined

onMounted(() => {
  currentTimeTimer = window.setInterval(() => {
    currentTimeMs.value = Date.now()
  }, 60_000)
})

onUnmounted(() => {
  if (currentTimeTimer !== undefined) {
    window.clearInterval(currentTimeTimer)
  }
})

const obligationOrders = computed(() => store.runningOrders)
const upcomingExpiryOrders = computed(() =>
  obligationOrders.value
    .filter(order => new Date(order.expiryAt).getTime() > currentTimeMs.value)
    .sort((left, right) => new Date(left.expiryAt).getTime() - new Date(right.expiryAt).getTime()),
)
const balances = computed(() => presentAccountBalances(store.account?.balances ?? []))
const balanceBreakdown = computed<PnlBreakdownItem[]>(() => buildBalanceBreakdown())

function buildBalanceBreakdown(): PnlBreakdownItem[] {
  const rows: PnlBreakdownItem[] = []
  for (const balance of balances.value) {
    if (!balance.usdtValueKnown) {
      // 后端无法定价的资产不能参与比例图；账户概览仍会显示“暂无估值”。
      continue
    }
    rows.push({
      key: balance.asset,
      label: balance.asset,
      value: balance.usdtValue,
    })
  }
  return rows
}
</script>

<template>
  <div class="view-grid">
    <Panel title="账户概览" class="span-4">
      <AccountBalanceList :account="store.account" class="large" empty-text="暂无持仓" />
    </Panel>

    <Panel title="义务仓概览" class="span-4">
      <div class="obligation-list">
        <article>
          <strong>卖出看跌义务仓</strong>
          <span>{{ store.portfolio?.putCount ?? 0 }} 个 · {{ store.portfolio ? decimal(store.portfolio.putQuantity) : '--' }} BTC</span>
          <em>已成交名义本金 {{ store.portfolio ? money(store.portfolio.putNotional) : '--' }}</em>
        </article>
        <article>
          <strong>卖出看涨义务仓</strong>
          <span>{{ store.portfolio?.callCount ?? 0 }} 个</span>
          <em>已成交数量 {{ store.portfolio ? decimal(store.portfolio.callQuantity) : '--' }} BTC</em>
        </article>
      </div>
    </Panel>

    <Panel title="资金与保证金" class="span-4">
      <DonutChart
        :items="balanceBreakdown"
        :total-value="store.account?.updatedAt ? store.account.totalUsdtValue : null"
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
              <th>已收权利金</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="order in obligationOrders" :key="order.id">
              <td>BTC</td>
              <td>{{ order.direction === 'sellPut' ? '卖出看跌（义务）' : '卖出看涨（义务）' }}</td>
              <td>{{ decimal(order.filledQuantity) }} BTC</td>
              <td>{{ money(order.strike, 'USDT', 0) }}</td>
              <td>{{ localDateTimeLabel(order.expiryAt) }}</td>
              <td class="positive">{{ money(order.premium) }}</td>
              <td>{{ order.status === 'partiallyFilled' ? '部分成交' : '运行中' }}</td>
            </tr>
            <tr
              v-for="balance in balances"
              :key="balance.asset"
            >
              <td>{{ balance.asset }}</td>
              <td>
                <NTooltip v-if="balance.details.length > 1" trigger="hover" placement="right">
                  <template #trigger>
                    <span class="balance-source-label" tabindex="0" :aria-label="balance.detailLabel">
                      {{ balance.sourceLabel }}
                    </span>
                  </template>
                  <div class="balance-tooltip">
                    <div v-for="detail in balance.details" :key="detail.key" class="balance-tooltip-row">
                      <strong>{{ detail.source }} {{ detail.asset }}</strong>
                      <span>
                        数量 {{ decimal(detail.free) }}
                        <template v-if="decimalStringHasValue(detail.locked)">
                          / 锁定 {{ decimal(detail.locked) }}
                        </template>
                      </span>
                      <em>估值 {{ detail.usdtValueKnown ? money(detail.usdtValue) : '暂无估值' }}</em>
                    </div>
                  </div>
                </NTooltip>
                <span v-else>{{ balance.sourceLabel }}</span>
              </td>
              <td>
                {{ decimal(balance.free) }}
                <span v-if="decimalStringHasValue(balance.locked)" class="locked-quantity">
                  / 锁定 {{ decimal(balance.locked) }}
                </span>
              </td>
              <td>--</td>
              <td>--</td>
              <td>--</td>
              <td>持有中</td>
            </tr>
          </tbody>
        </table>
        <p v-if="obligationOrders.length === 0 && balances.length === 0" class="empty-state">暂无持仓明细</p>
      </div>
    </Panel>

    <Panel title="到期日历" class="span-4">
      <div class="calendar-list">
        <p v-if="upcomingExpiryOrders.length === 0" class="empty-state">暂无未到期订单</p>
        <article v-for="order in upcomingExpiryOrders" :key="order.id">
          <strong>{{ localDateTimeLabel(order.expiryAt) }}</strong>
          <span>{{ order.direction === 'sellPut' ? '卖出看跌' : '卖出看涨' }}</span>
          <em>{{ order.filledQuantity }} BTC</em>
        </article>
      </div>
    </Panel>
  </div>
</template>

<style scoped>
.balance-source-label {
  border-bottom: 1px dashed var(--muted);
  color: var(--text);
  cursor: help;
}

.locked-quantity {
  color: var(--muted);
  font-size: 12px;
}
</style>
