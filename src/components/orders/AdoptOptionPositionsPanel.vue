<!--
Binance 期权持仓接管面板。

这里仅展示交易所当前义务仓和本地可接管判断。接管入口不下单、不移动资金，但接管后的
订单会继续进入同步、到期结算和必要的现货补齐流程，因此仍要求显式确认。
-->
<script setup lang="ts">
import type { AdoptableOptionPositionView } from '@/types/domain'

import { DownloadCloud, RefreshCw } from '@lucide/vue'
import { NButton, NCheckbox, NIcon, NTag } from 'naive-ui'
import { computed, shallowRef, watch } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { messageText, t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import { localDateTimeLabel } from '@/utils/date'

const store = useAppStore()
const { directionText, directionTone, money } = useFormat()
const selectedSymbols = shallowRef<string[]>([])

const rows = computed(() => store.adoptableOptionPositions)
const adoptableRows = computed(() => rows.value.filter(row => row.adoptable))
const selectedCount = computed(() => selectedSymbols.value.length)
const canAdoptSelected = computed(() => selectedCount.value > 0 && !store.isLoading('adopt'))

watch(rows, (next) => {
  // 刷新 Binance 持仓后移除已不可接管的旧选择，避免提交过期 symbol。
  const selectable = new Set(next.filter(row => row.adoptable).map(row => row.symbol))
  selectedSymbols.value = selectedSymbols.value.filter(symbol => selectable.has(symbol))
})

function rowChecked(row: AdoptableOptionPositionView) {
  return selectedSymbols.value.includes(row.symbol)
}

function updateRowChecked(row: AdoptableOptionPositionView, checked: boolean) {
  if (!row.adoptable) {
    return
  }
  selectedSymbols.value = checked
    ? [...new Set([...selectedSymbols.value, row.symbol])]
    : selectedSymbols.value.filter(symbol => symbol !== row.symbol)
}

function toggleAll(checked: boolean) {
  selectedSymbols.value = checked ? adoptableRows.value.map(row => row.symbol) : []
}

function refreshPositions() {
  void store.loadAdoptableOptionPositions()
}

function adoptSelected() {
  if (!canAdoptSelected.value) {
    return
  }
  if (!window.confirm(t('ui.takeOverTheSelectedBinanceOptionPositions'))) {
    return
  }
  // confirmLive 表示用户确认接管真实 Binance 持仓；这里只写入本地跟踪，不重放历史下单。
  void store.adoptOptionPositions({
    symbols: selectedSymbols.value,
    confirmLive: true,
  })
}
</script>

<template>
  <Panel :title="t('ui.takeOverBinancePositions')">
    <template #action>
      <div class="button-row compact adopt-actions">
        <NButton size="small" secondary :loading="store.isLoading('adopt')" @click="refreshPositions">
          <template #icon><NIcon :component="RefreshCw" /></template> {{ t('ui.queryPositions') }}
        </NButton>
        <NButton size="small" type="primary" :disabled="!canAdoptSelected" @click="adoptSelected">
          <template #icon><NIcon :component="DownloadCloud" /></template> {{ t('ui.takeOverSelected') }}
        </NButton>
      </div>
    </template>

    <div class="table-wrap">
      <table class="data-table adopt-table">
        <thead>
          <tr>
            <th>
              <NCheckbox
                :checked="adoptableRows.length > 0 && selectedCount === adoptableRows.length"
                :indeterminate="selectedCount > 0 && selectedCount < adoptableRows.length"
                :disabled="adoptableRows.length === 0"
                @update:checked="toggleAll"
              />
            </th>
            <th> {{ t('ui.contract') }} </th>
            <th> {{ t('ui.direction') }} </th>
            <th> {{ t('ui.expiryTime') }} </th>
            <th> {{ t('ui.strikePrice') }} </th>
            <th> {{ t('ui.contractsHeld') }} </th>
            <th> {{ t('ui.averageEntryPrice') }} </th>
            <th> {{ t('ui.markPrice') }} </th>
            <th> {{ t('ui.premium2') }} </th>
            <th> {{ t('ui.status') }} </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.symbol">
            <td>
              <NCheckbox
                :checked="rowChecked(row)"
                :disabled="!row.adoptable"
                @update:checked="checked => updateRowChecked(row, checked)"
              />
            </td>
            <td>{{ row.symbol }}</td>
            <td>
              <NTag :type="directionTone(row.direction)" size="small">
                {{ directionText(row.direction) }}
              </NTag>
            </td>
            <td>{{ localDateTimeLabel(row.expiryAt) }}</td>
            <td>{{ money(row.strike, 'USDT', 0) }}</td>
            <td>{{ row.quantity }}</td>
            <td>{{ money(row.entryPrice) }}</td>
            <td>{{ money(row.markPrice) }}</td>
            <td>{{ money(row.premium) }}</td>
            <td>
              <NTag :type="row.adoptable ? 'success' : row.alreadyTracked ? 'info' : 'warning'" size="small">
                {{ messageText(row.reason) }}
              </NTag>
            </td>
          </tr>
        </tbody>
      </table>
      <p v-if="rows.length === 0" class="empty-state"> {{ t('ui.noBinanceOptionPositionsToDisplay') }} </p>
    </div>
  </Panel>
</template>

<style scoped>
.adopt-actions {
  margin-top: 0;
}

.adopt-table {
  min-width: 1040px;
}

.adopt-table th:first-child,
.adopt-table td:first-child {
  width: 44px;
  text-align: center;
}
</style>
