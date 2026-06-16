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
import { t } from '@/i18n'
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
    <Panel :title="t('ui.accountOverview')" class="span-4">
      <AccountBalanceList :account="store.account" class="large" :empty-text="t('ui.noPositions')" />
    </Panel>

    <Panel :title="t('ui.shortOptionObligations')" class="span-4">
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
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th> {{ t('ui.underlying') }} </th>
              <th> {{ t('ui.positionType') }} </th>
              <th> {{ t('ui.quantity') }} </th>
              <th> {{ t('ui.averageStrikePrice') }} </th>
              <th> {{ t('ui.expiryDate') }} </th>
              <th> {{ t('ui.premiumReceived') }} </th>
              <th> {{ t('ui.status') }} </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="order in obligationOrders" :key="order.id">
              <td>BTC</td>
              <td>{{ order.direction === 'sellPut' ? t('ui.shortPutObligation') : t('ui.shortCallObligation') }}</td>
              <td>{{ decimal(order.filledQuantity) }} BTC</td>
              <td>{{ money(order.strike, 'USDT', 0) }}</td>
              <td>{{ localDateTimeLabel(order.expiryAt) }}</td>
              <td class="positive">{{ money(order.premium) }}</td>
              <td>{{ order.status === 'partiallyFilled' ? t('ui.partiallyFilled') : t('ui.running') }}</td>
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
                      <span> {{ t('ui.quantity') }} {{ decimal(detail.free) }}
                        <template v-if="decimalStringHasValue(detail.locked)"> {{ t('ui.locked') }} {{ decimal(detail.locked) }}
                        </template>
                      </span>
                      <em> {{ t('ui.value') }} {{ detail.usdtValueKnown ? money(detail.usdtValue) : t('ui.noValuation') }}</em>
                    </div>
                  </div>
                </NTooltip>
                <span v-else>{{ balance.sourceLabel }}</span>
              </td>
              <td>
                {{ decimal(balance.free) }}
                <span v-if="decimalStringHasValue(balance.locked)" class="locked-quantity"> {{ t('ui.locked') }} {{ decimal(balance.locked) }}
                </span>
              </td>
              <td>--</td>
              <td>--</td>
              <td>--</td>
              <td> {{ t('ui.holding') }} </td>
            </tr>
          </tbody>
        </table>
        <p v-if="obligationOrders.length === 0 && balances.length === 0" class="empty-state"> {{ t('ui.noPositionDetails') }} </p>
      </div>
    </Panel>

    <Panel :title="t('ui.expiryCalendar')" class="span-4">
      <div class="calendar-list">
        <p v-if="upcomingExpiryOrders.length === 0" class="empty-state"> {{ t('ui.noUnexpiredOrders') }} </p>
        <article v-for="order in upcomingExpiryOrders" :key="order.id">
          <strong>{{ localDateTimeLabel(order.expiryAt) }}</strong>
          <span>{{ order.direction === 'sellPut' ? t('ui.sellPut2') : t('ui.sellCall2') }}</span>
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
