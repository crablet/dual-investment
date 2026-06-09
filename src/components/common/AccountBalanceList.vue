<!--
账户余额聚合列表。

展示 Rust 已聚合的同名资产，hover 展示真实来源数量，并注明缓存时间和查询错误。
-->
<script setup lang="ts">
import type { AccountSnapshot, DecimalString } from '@/types/domain'
import { NAlert, NTooltip } from 'naive-ui'
import { computed } from 'vue'
import { useFormat } from '@/composables/useFormat'
import { presentAccountBalances } from '@/utils/balances'
import { localDateTimeLabel } from '@/utils/date'
import { isZeroDecimalString } from '@/utils/decimal'

const props = withDefaults(defineProps<{
  account?: AccountSnapshot
  emptyText?: string
}>(), {
  emptyText: '暂无资产',
})

const { decimal, money } = useFormat()

const balances = computed(() => presentAccountBalances(props.account?.balances ?? []))

function valuationText(usdtValue: DecimalString, known: boolean): string {
  return known ? money(usdtValue) : '暂无估值'
}
</script>

<template>
  <div class="account-list">
    <p class="account-snapshot-status">
      {{ props.account?.updatedAt ? `账户更新于 ${localDateTimeLabel(props.account.updatedAt)}` : '账户数据尚未更新' }}
      <span v-if="props.account?.unpricedAssetCount"> · {{ props.account.unpricedAssetCount }} 项资产暂无估值</span>
    </p>
    <NAlert v-if="props.account?.error" type="warning" class="account-cache-error">
      账户刷新失败：{{ props.account.error }}
      <template v-if="props.account.updatedAt">；保留上次成功查询的数据。</template>
    </NAlert>
    <p v-if="balances.length === 0 && props.account?.updatedAt" class="empty-state">{{ props.emptyText }}</p>
    <div v-for="balance in balances" :key="balance.asset" class="account-row">
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

<style scoped>
.account-snapshot-status,
.account-cache-error {
  grid-column: 1 / -1;
  margin: 0 0 8px;
}

.account-snapshot-status {
  color: var(--muted);
  font-size: 12px;
}
</style>
