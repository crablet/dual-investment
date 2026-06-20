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
  <div class="opportunity-table" role="region" tabindex="0" :aria-label="t('ui.candidateFilters')">
    <table :aria-label="t('ui.optionContract')">
      <thead>
        <tr>
          <th
            v-for="column in candidateSortColumns"
            :key="column.key"
            scope="col"
            :aria-sort="props.sortKey === column.key ? (props.sortDirection === 'asc' ? 'ascending' : 'descending') : undefined"
          >
            <button
              class="opportunity-sort"
              type="button"
              @click="emit('toggleSort', column.key)"
            >
              {{ column.label }}
              <NIcon
                aria-hidden="true"
                :component="props.sortKey === column.key
                  ? (props.sortDirection === 'asc' ? ChevronUp : ChevronDown)
                  : ArrowUpDown"
              />
            </button>
          </th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="candidate in props.candidates"
          :key="candidate.optionSymbol"
          class="opportunity-row"
          :class="{ selected: props.selectedOptionSymbol === candidate.optionSymbol }"
          @click="emit('select', candidate)"
        >
          <td>
            <button
              type="button"
              class="opportunity-pick"
              :aria-label="candidate.optionSymbol"
              :aria-pressed="props.selectedOptionSymbol === candidate.optionSymbol"
              @click.stop="emit('select', candidate)"
            >
              <span class="opportunity-strike">
                <strong>{{ money(candidate.strike, 'USDT', 0) }}</strong>
                <NTag
                  v-if="props.heldOptionSymbols.has(candidate.optionSymbol)"
                  size="small"
                  type="warning"
                  :bordered="false"
                  :title="t('ui.openShortPositions')"
                >{{ t('ui.position') }}</NTag>
              </span>
              <small>{{ t('ui.fromSpot') }} {{ signedPercent(candidate.strikeDistancePercent) }}</small>
            </button>
          </td>
          <td class="positive">{{ percent(candidate.annualized) }}</td>
          <td>
            <strong>{{ localDateTimeLabel(candidate.expiryAt) }}</strong>
            <small>{{ daysUntil(candidate.expiryAt) }} {{ t('ui.days') }}</small>
          </td>
          <td>{{ money(candidate.limitPrice, 'USDT', 0) }}</td>
          <td>{{ percent(candidate.exerciseProbability) }}</td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.opportunity-table {
  overflow-x: auto;
}
table {
  width: 100%;
  min-width: 566px;
  border-spacing: 0;
  table-layout: fixed;
}
th,
td {
  padding: 7px 12px;
  text-align: end;
}
th:first-child,
td:first-child,
th:nth-child(3),
td:nth-child(3) {
  text-align: start;
}
th:first-child {
  width: 24%;
  border-radius: 6px 0 0 0;
}
th:nth-child(2) {
  width: 14%;
}
th:nth-child(3) {
  width: 29%;
}
th:last-child {
  border-radius: 0 6px 0 0;
}
th {
  color: var(--muted);
  background: var(--inset);
  font-size: 12px;
  font-weight: 500;
}
td {
  height: 58px;
  border-bottom: 1px solid var(--line);
}
.opportunity-row {
  cursor: pointer;
}
.opportunity-row:hover {
  background: var(--raised);
}
.opportunity-row.selected {
  background: var(--gold-tint);
}
.opportunity-row.selected td:first-child {
  box-shadow: inset 2px 0 var(--gold);
}
.opportunity-sort,
.opportunity-pick {
  padding: 0;
  border: 0;
  color: inherit;
  background: transparent;
  font: inherit;
  text-align: inherit;
  cursor: pointer;
}
.opportunity-sort {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  line-height: 1.35;
}
.opportunity-sort .n-icon {
  flex-shrink: 0;
  font-size: 13px;
}
.opportunity-sort:hover,
[aria-sort] .opportunity-sort {
  color: var(--gold);
}
.opportunity-pick {
  display: block;
  width: 100%;
}
.opportunity-row strong {
  font-weight: 550;
}
.opportunity-row .positive {
  font-size: 16px;
  font-weight: 650;
}
.opportunity-row small {
  display: block;
  margin-top: 2px;
  color: var(--muted);
  font-size: 12px;
}
.opportunity-strike {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 8px;
}
.opportunity-strike strong {
  white-space: nowrap;
}
@container workspace (max-width: 1060px) {
  th,
  td {
    padding-inline: 8px;
  }
  th:first-child {
    width: 26%;
  }
  th:nth-child(3) {
    width: 27%;
  }
}
</style>
