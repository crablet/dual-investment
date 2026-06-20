<!-- 总览中的卖方持仓摘要；合约和交割详情在策略订单页查看。 -->
<script setup lang="ts">
import type { StrategyDirection, TrackedPosition } from '@/types/domain'

import { ArrowDownLeft, ArrowUpRight } from '@lucide/vue'
import { useFormat } from '@/composables/useFormat'
import { t } from '@/i18n'
import { localDateTimeLabel } from '@/utils/date'
import { isNegativeDecimalString } from '@/utils/decimal'

const props = defineProps<{
  positions: TrackedPosition[]
}>()

const { money } = useFormat()

function strategyLabel(direction: StrategyDirection) {
  return direction === 'sellPut' ? t('ui.buyBtcLow') : t('ui.sellBtcHigh2')
}
</script>

<template>
  <div v-if="props.positions.length" class="active-strategy-list" role="region" :aria-label="t('ui.openShortPositions')" tabindex="0">
    <table class="position-ledger">
      <thead>
        <tr>
          <th scope="col">{{ t('ui.strategy') }} / {{ t('ui.expiry') }}</th>
          <th scope="col">{{ t('ui.targetPrice') }}</th>
          <th scope="col">{{ t('ui.actualPosition') }}</th>
          <th scope="col">{{ t('ui.optionUnrealized') }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="position in props.positions" :key="position.optionSymbol">
          <td>
            <div class="position-identity">
              <span class="position-arrow" :class="{ call: position.direction === 'sellCall' }">
                <component :is="position.direction === 'sellPut' ? ArrowDownLeft : ArrowUpRight" :size="17" aria-hidden="true" />
              </span>
              <div>
                <strong>{{ strategyLabel(position.direction) }}</strong>
                <time :datetime="position.expiryAt">{{ localDateTimeLabel(position.expiryAt) }}</time>
              </div>
            </div>
          </td>
          <td>{{ money(position.strike, 'USDT', 0) }}</td>
          <td>{{ t('ui.contracts', { p0: position.quantity }) }}</td>
          <td :class="isNegativeDecimalString(position.unrealizedPnl) ? 'negative' : 'positive'">{{ money(position.unrealizedPnl) }}</td>
        </tr>
      </tbody>
    </table>
  </div>
  <p v-else class="empty-state">{{ t('ui.noShortPositions') }}</p>
</template>
