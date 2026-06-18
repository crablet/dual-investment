<!--
总览视图。

组合顶部指标、账户、活跃策略和事件日历；具体数据都来自 Pinia 快照派生。
-->
<script setup lang="ts">
import { computed } from 'vue'

import AccountBalanceList from '@/components/common/AccountBalanceList.vue'
import EventCalendarBoard from '@/components/common/EventCalendarBoard.vue'
import MetricCard from '@/components/common/MetricCard.vue'
import Panel from '@/components/common/Panel.vue'
import PositionSyncFeedback from '@/components/common/PositionSyncFeedback.vue'
import ActivePositionList from '@/components/overview/ActivePositionList.vue'
import { t } from '@/i18n'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

// 合约净仓只展示一次，不按本机订单历史累计义务。
const activePositions = computed(() =>
  [...store.openPositions].sort((left, right) => new Date(left.expiryAt).getTime() - new Date(right.expiryAt).getTime()),
)
</script>

<template>
  <div class="view-grid">
    <MetricCard v-for="metric in store.metrics" :key="metric.id" :metric="metric" />

    <Panel :title="t('ui.accountOverview')" class="span-6">
      <AccountBalanceList :account="store.account" />
    </Panel>

    <Panel :title="t('ui.openShortPositions')" class="span-6">
      <PositionSyncFeedback :sync="store.positionSync" />
      <ActivePositionList :positions="activePositions" />
    </Panel>

    <Panel :title="t('ui.eventCalendar')" class="span-12">
      <EventCalendarBoard />
    </Panel>
  </div>
</template>
