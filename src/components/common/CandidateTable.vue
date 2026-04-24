<script setup lang="ts">
// 候选机会表格。
// 用于总览页的快捷候选展示；真实下单仍由父组件弹窗确认后调用 store。
import { NButton, NTag } from 'naive-ui'
import { useFormat } from '@/composables/useFormat'
import type { StrategyCandidate } from '@/types/domain'
import { localDateTimeLabel } from '@/utils/date'

defineProps<{
  candidates: StrategyCandidate[]
}>()

const emit = defineEmits<{
  order: [candidate: StrategyCandidate]
}>()

const { directionText, directionTone, money, percent } = useFormat()
</script>

<template>
  <div class="table-wrap">
    <table class="data-table">
      <thead>
        <tr>
          <th>方向</th>
          <th>到期日</th>
          <th>执行价</th>
          <th>标记价格</th>
          <th>权利金</th>
          <th>年化</th>
          <th>被行权概率</th>
          <th>保本价</th>
          <th>操作</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="candidate in candidates" :key="candidate.id">
          <td>
            <NTag :type="directionTone(candidate.direction)" size="small">
              {{ directionText(candidate.direction) }}
            </NTag>
          </td>
          <td>{{ localDateTimeLabel(candidate.expiryAt) }}</td>
          <td>{{ money(candidate.strike) }}</td>
          <td>{{ money(candidate.limitPrice) }}</td>
          <td>{{ money(candidate.premium) }}</td>
          <td class="positive">{{ percent(candidate.annualized) }}</td>
          <td class="warning">{{ percent(candidate.exerciseProbability) }}</td>
          <td>{{ money(candidate.breakeven) }}</td>
          <td>
            <NButton size="small" type="primary" secondary @click="emit('order', candidate)">
              一键下单
            </NButton>
          </td>
        </tr>
      </tbody>
    </table>
    <p v-if="candidates.length === 0" class="empty-state">暂无候选机会</p>
  </div>
</template>
