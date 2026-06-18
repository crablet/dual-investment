<!--
账户卖方净仓摘要。

只展示用户在总览中判断当前期权义务所需的关键事实；订单号、合约代码和结算细节
留在订单页核对。
-->
<script setup lang="ts">
import type { StrategyDirection, TrackedPosition } from '@/types/domain'

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
  <div
    v-if="props.positions.length > 0"
    class="active-strategy-list"
    role="region"
    :aria-label="t('ui.openShortPositions')"
    tabindex="0"
  >
    <article v-for="position in props.positions" :key="position.optionSymbol" class="active-strategy-row">
      <header class="active-strategy-identity">
        <div class="active-strategy-title">
          <span
            class="active-strategy-marker"
            :class="`direction-${position.direction}`"
            aria-hidden="true"
          />
          <strong>{{ strategyLabel(position.direction) }}</strong>
        </div>
        <time :datetime="position.expiryAt">{{ localDateTimeLabel(position.expiryAt) }} {{ t('ui.expiry') }} </time>
      </header>

      <dl class="active-strategy-fact">
        <dt> {{ t('ui.targetPrice') }} </dt>
        <dd>{{ money(position.strike, 'USDT', 0) }}</dd>
      </dl>

      <dl class="active-strategy-fact">
        <dt> {{ t('ui.actualPosition') }} </dt>
        <dd>{{ t('ui.contracts', { p0: position.quantity }) }}</dd>
      </dl>

      <dl class="active-strategy-fact active-strategy-return">
        <dt> {{ t('ui.optionUnrealized') }} </dt>
        <dd :class="{ negative: isNegativeDecimalString(position.unrealizedPnl) }">{{ money(position.unrealizedPnl) }}</dd>
      </dl>
    </article>
  </div>

  <div v-else class="active-strategy-empty">
    <strong> {{ t('ui.noShortPositions') }} </strong>
  </div>
</template>

<style scoped>
.active-strategy-list {
  container-type: inline-size;
  min-width: 0;
  max-height: clamp(220px, 38vh, 360px);
  overflow-y: auto;
  scrollbar-gutter: stable;
}

.active-strategy-list:focus-visible {
  outline: 2px solid var(--gold);
  outline-offset: 2px;
}

.active-strategy-row {
  display: grid;
  grid-template-columns:
    minmax(158px, 1.35fr)
    minmax(120px, 0.95fr)
    minmax(100px, 0.75fr)
    minmax(132px, 1fr);
  gap: 14px;
  align-items: center;
  min-width: 0;
  padding: 13px 4px;
  border-top: 1px solid var(--line);
}

.active-strategy-row:last-child {
  border-bottom: 1px solid var(--line);
}

.active-strategy-identity,
.active-strategy-title,
.active-strategy-fact {
  min-width: 0;
}

.active-strategy-title {
  display: flex;
  gap: 7px;
  align-items: center;
}

.active-strategy-title strong {
  overflow: hidden;
  font-size: 14px;
  line-height: 1.35;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.active-strategy-marker {
  width: 8px;
  height: 8px;
  flex: 0 0 8px;
  border-radius: 999px;
}

.direction-sellPut {
  background: var(--green);
}

.direction-sellCall {
  background: var(--red);
}

.active-strategy-identity time {
  display: block;
  margin-top: 4px;
  color: var(--muted);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  line-height: 1.35;
  overflow-wrap: anywhere;
}

.active-strategy-fact {
  display: grid;
  gap: 3px;
  margin: 0;
}

.active-strategy-fact dt,
.active-strategy-fact dd {
  min-width: 0;
  margin: 0;
}

.active-strategy-fact dt {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.35;
}

.active-strategy-fact dd {
  font-size: 14px;
  font-variant-numeric: tabular-nums;
  font-weight: 650;
  line-height: 1.35;
  overflow-wrap: anywhere;
}

.active-strategy-return > dd {
  color: var(--green);
}

.active-strategy-return > dd.negative {
  color: var(--red);
}

.active-strategy-empty {
  display: grid;
  place-content: center;
  gap: 5px;
  min-height: 142px;
  padding: 24px;
  text-align: center;
}

.active-strategy-empty strong {
  font-size: 14px;
}

.active-strategy-empty span {
  max-width: 36ch;
  color: var(--muted);
  font-size: 13px;
  line-height: 1.5;
  text-wrap: pretty;
}

@container (max-width: 680px) {
  .active-strategy-row {
    grid-template-columns:
      minmax(142px, 1.2fr)
      minmax(90px, 0.8fr)
      minmax(80px, 0.7fr)
      minmax(110px, 1fr);
    gap: 10px;
  }
}

@container (max-width: 480px) {
  .active-strategy-row {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px 18px;
    padding-block: 15px;
  }
}

@container (max-width: 360px) {
  .active-strategy-row {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
