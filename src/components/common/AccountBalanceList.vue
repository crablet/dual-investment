<!--
账户余额聚合列表。

把现货、期权保证金和 Simple Earn 同名资产按 ticker 汇总，hover 展示真实来源数量。
-->
<script setup lang="ts">
import type { BalanceRow, DecimalString } from '@/types/domain'
import { NTooltip } from 'naive-ui'
import { computed } from 'vue'
import { useFormat } from '@/composables/useFormat'
import { aggregateBalances } from '@/utils/balances'
import { isZeroDecimalString } from '@/utils/decimal'

const props = withDefaults(defineProps<{
  balances: BalanceRow[]
  emptyText?: string
}>(), {
  emptyText: '暂无资产',
})

const { decimal, money } = useFormat()

const aggregatedBalances = computed(() => aggregateBalances(props.balances))

function valuationText(usdtValue: DecimalString, known: boolean): string {
  return known ? money(usdtValue) : '暂无估值'
}
</script>

<template>
  <div class="account-list">
    <p v-if="aggregatedBalances.length === 0" class="empty-state">{{ props.emptyText }}</p>
    <div v-for="balance in aggregatedBalances" :key="balance.asset" class="account-row">
      <span class="account-asset">
        <strong>{{ balance.asset }}</strong>
      </span>
      <NTooltip trigger="hover" placement="right">
        <template #trigger>
          <span
            class="account-values account-values--hover"
            tabindex="0"
            :aria-label="balance.detailLabel"
          >
            <strong>{{ decimal(balance.free) }}</strong>
            <em>{{ valuationText(balance.usdtValue, balance.usdtValueKnown) }}</em>
          </span>
        </template>
        <div class="balance-tooltip">
          <div v-for="detail in balance.details" :key="detail.key" class="balance-tooltip-row">
            <strong>{{ detail.source }} {{ detail.asset }}</strong>
            <span>
              数量 {{ decimal(detail.free) }}
              <template v-if="!isZeroDecimalString(detail.locked)">
                / 锁定 {{ decimal(detail.locked) }}
              </template>
            </span>
            <em>估值 {{ valuationText(detail.usdtValue, detail.usdtValueKnown) }}</em>
          </div>
        </div>
      </NTooltip>
    </div>
  </div>
</template>
