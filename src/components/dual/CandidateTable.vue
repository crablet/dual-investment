<!--
候选机会表格。

组件只展示父级传入的 Binance 实盘候选快照；排序切换和选中操作通过事件交回父级。
-->
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
import { NIcon } from 'naive-ui'
import {
  candidateSortColumns,
  daysUntil,
} from '@/composables/useDualInvestmentCandidates'
import { useFormat } from '@/composables/useFormat'
import { localDateTimeLabel } from '@/utils/date'
import { decimalStringToDisplayNumber } from '@/utils/decimal'

const props = defineProps<{
  candidates: StrategyCandidate[]
  selectedCandidateId?: string
  sortKey: CandidateSortKey
  sortDirection: CandidateSortDirection
}>()

const emit = defineEmits<{
  select: [candidate: StrategyCandidate]
  toggleSort: [key: CandidateSortKey]
}>()

const { money, percent } = useFormat()

function signedPercent(value: string | number) {
  const numeric = typeof value === 'number' ? value : decimalStringToDisplayNumber(value)
  if (!Number.isFinite(numeric))
    return '--'
  const sign = numeric > 0 ? '+' : ''
  return `${sign}${percent(numeric)}`
}
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
          :component="props.sortKey === column.key
            ? (props.sortDirection === 'asc' ? ChevronUp : ChevronDown)
            : ArrowUpDown"
        />
      </button>
    </div>
    <button
      v-for="candidate in props.candidates"
      :key="candidate.id"
      class="opportunity-row"
      :class="{ selected: props.selectedCandidateId === candidate.id }"
      type="button"
      @click="emit('select', candidate)"
    >
      <span>
        <strong>{{ money(candidate.strike) }}</strong>
        <em>
          距现货 {{ signedPercent(candidate.strikeDistancePercent) }}
        </em>
      </span>
      <span class="positive">{{ percent(candidate.annualized) }}</span>
      <span>
        <strong>{{ localDateTimeLabel(candidate.expiryAt) }}</strong>
        <em>{{ daysUntil(candidate.expiryAt) }} 天</em>
      </span>
      <span>{{ money(candidate.limitPrice) }}</span>
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
</style>
