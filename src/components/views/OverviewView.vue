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
import ActiveStrategyList from '@/components/overview/ActiveStrategyList.vue'
import { t } from '@/i18n'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

// 按到期时间展示全部活跃策略，不改变共享订单列表的顺序。
const activeStrategies = computed(() =>
  [...store.runningOrders].sort((left, right) => new Date(left.expiryAt).getTime() - new Date(right.expiryAt).getTime()),
)
</script>

<template>
  <div class="view-grid">
    <MetricCard v-for="metric in store.metrics" :key="metric.id" :metric="metric" />

    <Panel :title="t('ui.accountOverview')" class="span-6">
      <AccountBalanceList :account="store.account" />
    </Panel>

    <Panel :title="t('ui.activeStrategies2')" class="span-6">
      <ActiveStrategyList :orders="activeStrategies" />
    </Panel>

    <Panel :title="t('ui.eventCalendar')" class="span-12">
      <EventCalendarBoard />
    </Panel>
  </div>
</template>
