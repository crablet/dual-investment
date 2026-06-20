<!-- 按 Put / Call 展示 Rust 汇总的本机卖出权利金。 -->
<script setup lang="ts">
import { computed } from 'vue'

import BarList from '@/components/common/BarList.vue'
import PageHeading from '@/components/common/PageHeading.vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import { relativeBarWidth } from '@/utils/bars'
import { localDateKey } from '@/utils/date'
import { decimalStringToDisplayNumber, isPositiveDecimalString } from '@/utils/decimal'

const store = useAppStore()
const { directionText, money, percent } = useFormat()
const recordedOrders = computed(() => store.orders.filter(order => isPositiveDecimalString(order.filledQuantity)))

const strategyPerformance = computed(() => {
  const pnl = store.pnl
  if (!pnl)
    return []
  // 金额已经由 Rust 按方向汇总；number 只参与图形宽度计算。
  const rows = [
    { label: directionText('sellPut'), value: pnl.sellPutPremium },
    { label: directionText('sellCall'), value: pnl.sellCallPremium },
  ]
  const max = Math.max(...rows.map(row => decimalStringToDisplayNumber(row.value)))
  return rows.map(row => ({
    label: row.label,
    value: money(row.value),
    width: relativeBarWidth(decimalStringToDisplayNumber(row.value), max),
  }))
})
</script>

<template>
  <PageHeading :title="t('ui.returns')" />
  <div class="view-grid analytics-grid">
    <Panel :title="t('ui.recordedPremium')" class="span-6">
      <div v-if="store.pnl" class="summary-cards pnl-summary-cards">
        <article><span> {{ t('ui.totalPremium') }} </span><strong>{{ money(store.pnl.recordedPremium) }}</strong></article>
        <article><span> {{ t('ui.todayRecordedPremium') }} </span><strong>{{ money(store.pnl.todayRecordedPremium) }}</strong></article>
      </div>
      <p v-else class="empty-state">{{ t('ui.noReturnsToDisplay') }}</p>
    </Panel>

    <Panel :title="t('ui.premiumByStrategy')" class="span-6">
      <BarList v-if="store.pnl" :items="strategyPerformance" />
      <p v-else class="empty-state">{{ t('ui.noReturnsToDisplay') }}</p>
    </Panel>

    <Panel :title="t('ui.recordedPremiumDetails')" class="span-12">
      <div class="table-wrap">
        <table class="data-table" :aria-label="t('ui.recordedPremiumDetails')">
          <thead>
            <tr>
              <th scope="col"> {{ t('ui.date') }} </th>
              <th scope="col"> {{ t('ui.strategy') }} </th>
              <th class="numeric" scope="col"> {{ t('ui.premium2') }} </th>
              <th class="numeric" scope="col"> {{ t('ui.annualizedReturn') }} </th>
              <th scope="col"> {{ t('ui.notes') }} </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="order in recordedOrders" :key="order.id">
              <td>{{ localDateKey(order.createdAt) }}</td>
              <td>{{ directionText(order.direction) }}</td>
              <td class="positive numeric">{{ money(order.premium) }}</td>
              <td class="numeric">{{ percent(order.annualized) }}</td>
              <td>{{ order.optionSymbol }} {{ t('ui.premiumReceived2') }} </td>
            </tr>
          </tbody>
        </table>
        <p v-if="recordedOrders.length === 0" class="empty-state"> {{ t('ui.noReturnsToDisplay') }} </p>
      </div>
    </Panel>
  </div>
</template>
