<!-- 展示候选合约和卖方持仓标记，排序与选择交给父组件处理。 -->
<script setup lang="ts">
import type {
  CandidateSortDirection,
  CandidateSortKey,
} from '@/composables/useDualInvestmentCandidates'

import type { StrategyCandidate } from '@/types/domain'
import {
  ArrowUpDown,
  ChevronDown,
  ChevronUp,
} from '@lucide/vue'
import { NIcon, NTag } from 'naive-ui'
import {
  candidateSortColumns,
  daysUntil,
} from '@/composables/useDualInvestmentCandidates'
import { useFormat } from '@/composables/useFormat'
import { t } from '@/i18n'
import { localDateTimeLabel } from '@/utils/date'

const props = defineProps<{
  candidates: StrategyCandidate[]
  heldOptionSymbols: ReadonlySet<string>
  selectedOptionSymbol?: string
  sortKey: CandidateSortKey
  sortDirection: CandidateSortDirection
}>()

const emit = defineEmits<{
  select: [candidate: StrategyCandidate]
  toggleSort: [key: CandidateSortKey]
}>()

const { money, percent, signedPercent } = useFormat()
</script>

<template>
  <div class="opportunity-table">
    <div class="opportunity-row opportunity-head">
      <button
        v-for="column in candidateSortColumns"
        :key="column.key"
        class="opportunity-sort"
        :class="{ active: props.sortKey === column.key }"
        type="button"
        :aria-pressed="props.sortKey === column.key"
        @click="emit('toggleSort', column.key)"
      >
        <span>{{ column.label }}</span>
        <NIcon
          aria-hidden="true"
          :component="props.sortKey === column.key
            ? (props.sortDirection === 'asc' ? ChevronUp : ChevronDown)
            : ArrowUpDown"
        />
      </button>
    </div>
    <button
      v-for="candidate in props.candidates"
      :key="candidate.optionSymbol"
      class="opportunity-row"
      :class="{ selected: props.selectedOptionSymbol === candidate.optionSymbol }"
      :aria-pressed="props.selectedOptionSymbol === candidate.optionSymbol"
      type="button"
      @click="emit('select', candidate)"
    >
      <span>
        <span class="opportunity-strike">
          <strong>{{ money(candidate.strike, 'USDT', 0) }}</strong>
          <NTag
            v-if="props.heldOptionSymbols.has(candidate.optionSymbol)"
            size="small"
            type="warning"
            :bordered="false"
            :title="t('ui.openShortPositions')"
          > {{ t('ui.position') }} </NTag>
        </span>
        <em> {{ t('ui.fromSpot') }} {{ signedPercent(candidate.strikeDistancePercent) }}
        </em>
      </span>
      <span class="positive">{{ percent(candidate.annualized) }}</span>
      <span>
        <strong>{{ localDateTimeLabel(candidate.expiryAt) }}</strong>
        <em>{{ daysUntil(candidate.expiryAt) }} {{ t('ui.days') }} </em>
      </span>
      <span>{{ money(candidate.limitPrice, 'USDT', 0) }}</span>
      <span>{{ percent(candidate.exerciseProbability) }}</span>
    </button>
  </div>
</template>

<style scoped>
.opportunity-table {
  min-width: 0;
  overflow: auto;
}

.opportunity-row {
  display: grid;
  grid-template-columns: minmax(104px, 1fr) minmax(52px, 0.5fr) minmax(128px, 1.2fr) minmax(64px, 0.65fr) minmax(
      88px,
      0.85fr
    );
  align-items: center;
  gap: 10px;
  width: 100%;
  min-width: 100%;
  min-height: 72px;
  padding: 0 8px;
  border: 0;
  border-bottom: 1px solid var(--line);
  color: var(--text);
  text-align: left;
  background: transparent;
}

button.opportunity-row {
  cursor: pointer;
}

button.opportunity-row:hover,
button.opportunity-row.selected,
button.opportunity-row.selected:hover {
  background: rgba(245, 182, 37, 0.06);
}

.opportunity-head {
  min-height: 42px;
  color: var(--muted);
  font-size: 12px;
  font-weight: 700;
}

.opportunity-sort {
  display: inline-flex;
  align-items: center;
  justify-content: flex-start;
  gap: 6px;
  min-width: 0;
  padding: 0;
  border: 0;
  color: inherit;
  font: inherit;
  text-align: left;
  background: transparent;
  cursor: pointer;
}

.opportunity-sort span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.opportunity-sort .n-icon {
  flex: 0 0 auto;
  font-size: 14px;
  opacity: 0.55;
}

.opportunity-sort:hover,
.opportunity-sort.active {
  color: var(--text);
}

.opportunity-sort.active .n-icon {
  color: var(--gold);
  opacity: 1;
}

.opportunity-row strong,
.opportunity-row em {
  display: block;
}

.opportunity-row em {
  margin-top: 3px;
  color: var(--muted);
  font-style: normal;
  font-size: 12px;
}

.opportunity-strike {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 6px;
}

.opportunity-strike strong {
  white-space: nowrap;
}
</style>
