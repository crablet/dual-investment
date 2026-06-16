<!--
策略订单视图。

展示真实订单、同步/结算操作和恢复记录；恢复动作会回到 Rust 重新查询 Binance。
-->
<script setup lang="ts">
import type { StrategyOrder } from '@/types/domain'

import { CheckCircle2, RefreshCw, ScanLine, XCircle } from '@lucide/vue'
import { NButton, NIcon, NTag } from 'naive-ui'
import { computed, shallowRef, watchEffect } from 'vue'
import Panel from '@/components/common/Panel.vue'
import AdoptOptionPositionsPanel from '@/components/orders/AdoptOptionPositionsPanel.vue'
import { useFormat } from '@/composables/useFormat'
import { useLocalToday } from '@/composables/useLocalToday'
import { useRecoveryRows } from '@/composables/useRecoveryRows'
import { messageText, t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import { localDateKey, localDateTimeLabel } from '@/utils/date'

const store = useAppStore()
const { todayKey } = useLocalToday()
const { directionText, directionTone, money, statusText, statusTone } = useFormat()
const selectedId = shallowRef<string>()
const {
  recoveryRows,
  recoveryStatusLabel,
  recoveryStatusTone,
  canRecover,
  canManuallyResolve,
  recoverActionLabel,
  recoverRow,
  resolveFundingRow,
  finishSettlementRow,
} = useRecoveryRows()

interface OrderDetailRow {
  label: string
  value: string
}

watchEffect(() => {
  // 首次进入或订单列表刷新后，默认选中最新订单，保留用户已有选择。
  if (!selectedId.value && store.orders[0]) {
    selectedId.value = store.orders[0].id
  }
})

const selected = computed(() => store.orders.find(order => order.id === selectedId.value) ?? store.orders[0])
const todayExpiryCount = computed(() => {
  // “今日到期”按本地日期展示，真实结算触发仍以后端 expiryAt 毫秒时间戳为准。
  return store.orders.filter(order => localDateKey(order.expiryAt) === todayKey.value).length
})
const detailRows = computed(() => {
  if (!selected.value) {
    return []
  }
  return createOrderDetailRows(selected.value)
})

function createOrderDetailRows(order: StrategyOrder): OrderDetailRow[] {
  return [
    { label: t('ui.direction'), value: directionText(order.direction) },
    { label: t('ui.underlying'), value: order.symbol },
    { label: t('ui.optionContract'), value: order.optionSymbol },
    { label: t('ui.exchangeStatus'), value: order.exchangeStatus },
    { label: t('ui.clientOrderId'), value: order.clientOrderId },
    { label: t('ui.expiryTime'), value: localDateTimeLabel(order.expiryAt) },
    { label: t('ui.strikePrice'), value: money(order.strike, 'USDT', 0) },
    { label: t('ui.filledQuantity'), value: t('ui.contracts2', { p0: order.filledQuantity, p1: order.quantity }) },
    { label: t('ui.averageFillPrice'), value: money(order.avgPrice) },
    { label: t('ui.premium2'), value: money(order.premium) },
    { label: t('ui.breakevenPrice'), value: money(order.breakeven) },
    { label: t('ui.settlementReferencePrice'), value: money(order.settlementPrice) },
    { label: t('ui.spotOrderId'), value: order.spotOrderId ?? '--' },
    { label: t('ui.spotStatus'), value: order.spotOrderStatus || '--' },
    { label: t('ui.spotFilledQuantity'), value: `${order.spotFilledQuantity} BTC` },
    { label: t('ui.spotAverageFillPrice'), value: money(order.spotAvgPrice) },
    { label: t('ui.expiryRebalancing'), value: messageText(order.settlementAction) },
    { label: t('ui.rebalancingResult'), value: messageText(order.rebalanceMessage) || '--' },
  ]
}
</script>

<template>
  <div class="view-grid">
    <Panel :title="t('ui.orderStatistics')" class="span-12">
      <template #action>
        <div class="button-row compact">
          <NButton size="small" secondary :loading="store.isLoading('sync')" @click="store.syncLiveState()">
            <template #icon><NIcon :component="RefreshCw" /></template> {{ t('ui.syncOrders') }}
          </NButton>
          <NButton size="small" secondary :loading="store.isLoading('settlement')" @click="store.runSettlementScan()">
            <template #icon><NIcon :component="ScanLine" /></template> {{ t('ui.scanSettlements') }}
          </NButton>
        </div>
      </template>
      <div class="summary-cards order-summary-cards">
        <article><span> {{ t('ui.totalOrders') }} </span><strong>{{ store.orders.length }}</strong></article>
        <article><span> {{ t('ui.activeOrders') }} </span><strong>{{ store.runningOrders.length }}</strong></article>
        <article><span> {{ t('ui.expiringToday') }} </span><strong>{{ todayExpiryCount }}</strong></article>
        <article><span> {{ t('ui.totalPremium') }} </span><strong>{{ store.pnl ? money(store.pnl.optionPremium) : '--' }}</strong></article>
      </div>
    </Panel>

    <Panel :title="t('ui.orderList')" class="span-8">
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th> {{ t('ui.orderId') }} </th>
              <th> {{ t('ui.direction') }} </th>
              <th> {{ t('ui.underlying') }} </th>
              <th> {{ t('ui.strikePrice') }} </th>
              <th> {{ t('ui.expiryDate') }} </th>
              <th> {{ t('ui.quantity') }} </th>
              <th> {{ t('ui.premium2') }} </th>
              <th> {{ t('ui.status') }} </th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="order in store.orders"
              :key="order.id"
              :class="{ selected: order.id === selectedId }"
              @click="selectedId = order.id"
            >
              <td>{{ order.id }}</td>
              <td><NTag :type="directionTone(order.direction)" size="small">{{ directionText(order.direction) }}</NTag></td>
              <td>{{ order.symbol }}</td>
              <td>{{ money(order.strike, 'USDT', 0) }}</td>
              <td>{{ localDateTimeLabel(order.expiryAt) }}</td>
              <td>{{ order.filledQuantity }} / {{ order.quantity }}</td>
              <td>{{ money(order.premium) }}</td>
              <td><NTag :type="statusTone(order.status)" size="small">{{ statusText(order.status) }}</NTag></td>
            </tr>
          </tbody>
        </table>
        <p v-if="store.orders.length === 0" class="empty-state"> {{ t('ui.noOrders') }} </p>
      </div>
    </Panel>

    <Panel :title="t('ui.orderDetails')" class="span-4">
      <div v-if="selected" class="detail-list order-detail-list">
        <header class="order-detail-heading">
          <h3 :title="selected.id">{{ selected.id }}</h3>
          <NTag :type="statusTone(selected.status)" size="small" class="detail-status-tag">
            {{ statusText(selected.status) }}
          </NTag>
        </header>
        <p v-for="row in detailRows" :key="row.label">
          <span>{{ row.label }}</span>
          <strong :title="row.value">{{ row.value }}</strong>
        </p>
      </div>
      <p v-else class="empty-state"> {{ t('ui.noOrderDetails') }} </p>
    </Panel>

    <AdoptOptionPositionsPanel class="span-12" />

    <Panel :title="t('ui.recoveryCenter')" class="span-12">
      <div class="table-wrap">
        <table class="data-table recovery-table">
          <thead>
            <tr>
              <th> {{ t('ui.time') }} </th>
              <th> {{ t('ui.type') }} </th>
              <th> {{ t('ui.target') }} </th>
              <th> {{ t('ui.amountUnderlying') }} </th>
              <th> {{ t('ui.exchangeReference') }} </th>
              <th> {{ t('ui.status') }} </th>
              <th> {{ t('ui.sourceOfFacts') }} </th>
              <th> {{ t('ui.nextStep') }} </th>
              <th> {{ t('ui.description') }} </th>
              <th> {{ t('ui.action') }} </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in recoveryRows" :key="`${row.source}-${row.id}`">
              <td>{{ localDateTimeLabel(row.updatedAt) }}</td>
              <td>{{ row.type }}</td>
              <td>{{ row.target }}</td>
              <td>{{ row.amount }}</td>
              <td>{{ row.exchangeRef }}</td>
              <td>
                <NTag :type="recoveryStatusTone(row)" size="small">
                  {{ recoveryStatusLabel(row) }}
                </NTag>
              </td>
              <td>{{ row.factSource }}</td>
              <td>{{ row.nextAction }}</td>
              <td>
                <div>{{ row.message }}</div>
                <div v-if="row.diagnostics" class="recovery-diagnostics">
                  {{ row.diagnostics }}
                </div>
              </td>
              <td>
                <div class="button-row compact">
                  <NButton
                    v-if="row.source !== 'settlement'"
                    size="small"
                    secondary
                    :loading="store.isLoading('recover')"
                    :disabled="!canRecover(row)"
                    @click="recoverRow(row)"
                  >
                    <template #icon><NIcon :component="RefreshCw" /></template>
                    {{ recoverActionLabel(row) }}
                  </NButton>
                  <NButton
                    v-if="row.source === 'settlement' && row.canFinish"
                    size="small"
                    type="warning"
                    secondary
                    :loading="store.isLoading('recover')"
                    @click="finishSettlementRow(row)"
                  >
                    {{ t('ui.endEarnFinalization') }}
                  </NButton>
                  <template v-if="canManuallyResolve(row)">
                    <NButton
                      size="small"
                      type="success"
                      secondary
                      :loading="store.isLoading('recover')"
                      @click="resolveFundingRow(row, 'confirmed')"
                    >
                      <template #icon><NIcon :component="CheckCircle2" /></template> {{ t('ui.verifySuccess') }}
                    </NButton>
                    <NButton
                      size="small"
                      type="error"
                      secondary
                      :loading="store.isLoading('recover')"
                      @click="resolveFundingRow(row, 'failed')"
                    >
                      <template #icon><NIcon :component="XCircle" /></template> {{ t('ui.verifyFailure') }}
                    </NButton>
                  </template>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="recoveryRows.length === 0" class="empty-state"> {{ t('ui.noRecoveryRecords') }} </p>
      </div>
    </Panel>
  </div>
</template>

<style scoped>
.recovery-table td:nth-child(n + 7):not(:last-child) {
  min-width: 160px;
  max-width: 280px;
  white-space: normal;
  overflow-wrap: anywhere;
}

.recovery-table td:nth-child(9):not(:last-child) {
  min-width: 280px;
}

.recovery-table th:last-child,
.recovery-table td:last-child {
  position: sticky;
  right: 0;
  background: var(--surface);
}

.recovery-diagnostics {
  margin-top: 4px;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.5;
  overflow-wrap: anywhere;
}
</style>
