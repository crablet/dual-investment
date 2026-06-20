<script setup lang="ts">
import { NTag } from 'naive-ui'
import { computed, shallowRef } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { messageText, t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import { localDateTimeLabel } from '@/utils/date'
import { deliveryStatusText, deliveryStatusTone } from '@/utils/labels'

const store = useAppStore()
const { money, directionText } = useFormat()
const selectedId = shallowRef<string>()
const selected = computed(() => store.deliveries.find(task => task.id === selectedId.value) ?? store.deliveries[0])
const details = computed(() => {
  const task = selected.value
  if (!task)
    return []
  return [
    { label: t('ui.lastTrackedQuantity'), value: task.lastPositionQuantity },
    { label: t('ui.contractUnit'), value: `${task.unit} BTC` },
    { label: t('ui.settlementReferencePrice'), value: task.settlementPrice === null ? '--' : money(task.settlementPrice) },
    { label: t('ui.exerciseCashflow'), value: task.cashSettlement === null ? '--' : money(task.cashSettlement) },
    { label: t('ui.spotOrderId'), value: task.spotOrderId ?? '--' },
    { label: t('ui.clientOrderId'), value: task.spotClientOrderId || '--' },
    { label: t('ui.spotStatus'), value: task.spotOrderStatus || '--' },
    { label: t('ui.spotAverageFillPrice'), value: task.spotOrderId ? money(task.spotAvgPrice) : '--' },
  ]
})
</script>

<template>
  <Panel :title="t('ui.deliveryTasks')">
    <div class="table-wrap">
      <table class="data-table delivery-table" :aria-label="t('ui.deliveryTasks')">
        <thead>
          <tr>
            <th scope="col">{{ t('ui.optionContract') }}</th>
            <th scope="col">{{ t('ui.direction') }}</th>
            <th scope="col">{{ t('ui.expiryDate') }}</th>
            <th scope="col">{{ t('ui.confirmedDeliveryQuantity') }}</th>
            <th scope="col">{{ t('ui.spotFilledQuantity') }}</th>
            <th scope="col">{{ t('ui.deliveryStatus') }}</th>
            <th scope="col">{{ t('ui.description') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="task in store.deliveries"
            :key="task.id"
            :class="{ selected: selected?.id === task.id }"
            @click="selectedId = task.id"
          >
            <td>{{ task.optionSymbol }}</td>
            <td>{{ directionText(task.direction) }}</td>
            <td>{{ localDateTimeLabel(task.expiryAt) }}</td>
            <td>{{ task.quantity ?? t('ui.awaitingConfirmation') }}</td>
            <td>{{ task.spotFilledQuantity }} BTC</td>
            <td><NTag :type="deliveryStatusTone(task.status)" size="small">{{ deliveryStatusText(task.status) }}</NTag></td>
            <td class="delivery-message">{{ messageText(task.message) }}</td>
          </tr>
        </tbody>
      </table>
      <p v-if="store.deliveries.length === 0" class="empty-state">{{ t('ui.noDeliveryTasks') }}</p>
    </div>
    <dl v-if="selected" class="delivery-details">
      <div v-for="detail in details" :key="detail.label">
        <dt>{{ detail.label }}</dt>
        <dd>{{ detail.value }}</dd>
      </div>
    </dl>
  </Panel>
</template>

<style scoped>
.delivery-message {
  min-width: 220px;
  max-width: 340px;
  white-space: normal;
}
.delivery-details {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 14px 24px;
  margin: 18px 0 0;
  padding-top: 16px;
  border-top: 1px solid var(--line);
}
.delivery-details dt {
  color: var(--muted);
}
.delivery-details dd {
  margin: 4px 0 0;
  overflow-wrap: anywhere;
  font-variant-numeric: tabular-nums;
}
</style>
