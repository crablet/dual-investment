<!--
持仓与现货视图。

展示期权义务仓和现货资产；金额与数量已在 Rust 汇总，前端组织列表和图表。
-->
<script setup lang="ts">
import type { PnlBreakdownItem } from '@/types/view'

import { computed, onMounted, onUnmounted, shallowRef } from 'vue'
import AccountBalanceList from '@/components/common/AccountBalanceList.vue'
import DonutChart from '@/components/common/DonutChart.vue'
import Panel from '@/components/common/Panel.vue'
import PositionSyncFeedback from '@/components/common/PositionSyncFeedback.vue'
import { useFormat } from '@/composables/useFormat'
import { t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import { presentAccountBalances } from '@/utils/balances'
import { localDateTimeLabel } from '@/utils/date'

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

const positions = computed(() => store.openPositions)
const upcomingPositions = computed(() =>
  positions.value
    .filter(position => new Date(position.expiryAt).getTime() > currentTimeMs.value)
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
    <Panel :title="t('ui.accountOverview')" class="span-4">
      <AccountBalanceList :account="store.account" class="large" :empty-text="t('ui.noPositions')" />
    </Panel>

    <Panel :title="t('ui.openShortPositions')" class="span-4">
      <div class="obligation-list">
        <article>
          <strong> {{ t('ui.shortPutPositions') }} </strong>
          <span>{{ store.portfolio?.putCount ?? 0 }} {{ t('ui.positions') }} {{ store.portfolio ? decimal(store.portfolio.putQuantity) : '--' }} BTC</span>
          <em> {{ t('ui.filledNotionalPrincipal') }} {{ store.portfolio ? money(store.portfolio.putNotional) : '--' }}</em>
        </article>
        <article>
          <strong> {{ t('ui.shortCallPositions') }} </strong>
          <span>{{ store.portfolio?.callCount ?? 0 }} {{ t('ui.positions2') }} </span>
          <em> {{ t('ui.filledQuantity2') }} {{ store.portfolio ? decimal(store.portfolio.callQuantity) : '--' }} BTC</em>
        </article>
      </div>
    </Panel>

    <Panel :title="t('ui.fundsMargin')" class="span-4">
      <DonutChart
        :items="balanceBreakdown"
        :total-value="store.account?.updatedAt ? store.account.totalUsdtValue : null"
        :center-label="t('ui.assetValuation')"
        :empty-text="t('ui.noValuedAssets')"
      />
    </Panel>

    <Panel :title="t('ui.positionDetails')" class="span-8">
      <PositionSyncFeedback :sync="store.positionSync" />
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th> {{ t('ui.optionContract') }} </th>
              <th> {{ t('ui.positionType') }} </th>
              <th> {{ t('ui.netPositionQuantity') }} </th>
              <th> {{ t('ui.strikePrice') }} </th>
              <th> {{ t('ui.averageEntryPrice') }} </th>
              <th> {{ t('ui.markPrice') }} </th>
              <th> {{ t('ui.optionUnrealized') }} </th>
              <th> {{ t('ui.expiryDate') }} </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="position in positions" :key="position.optionSymbol">
              <td>{{ position.optionSymbol }}</td>
              <td>{{ position.direction === 'sellPut' ? t('ui.shortPutObligation') : t('ui.shortCallObligation') }}</td>
              <td>{{ position.quantity }}</td>
              <td>{{ money(position.strike, 'USDT', 0) }}</td>
              <td>{{ money(position.entryPrice) }}</td>
              <td>{{ money(position.markPrice) }}</td>
              <td>{{ money(position.unrealizedPnl) }}</td>
              <td>{{ localDateTimeLabel(position.expiryAt) }}</td>
            </tr>
          </tbody>
        </table>
        <p v-if="positions.length === 0" class="empty-state"> {{ t('ui.noShortPositions') }} </p>
      </div>
    </Panel>

    <Panel :title="t('ui.expiryCalendar')" class="span-4">
      <div class="calendar-list">
        <p v-if="upcomingPositions.length === 0" class="empty-state"> {{ t('ui.noUpcomingPositions') }} </p>
        <article v-for="position in upcomingPositions" :key="position.optionSymbol">
          <strong>{{ localDateTimeLabel(position.expiryAt) }}</strong>
          <span>{{ position.direction === 'sellPut' ? t('ui.sellPut2') : t('ui.sellCall2') }}</span>
          <em>{{ t('ui.contracts', { p0: position.quantity }) }}</em>
        </article>
      </div>
    </Panel>
  </div>
</template>
