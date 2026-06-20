<!-- 从 Pinia 快照展示账户、持仓、指标和日历。 -->
<script setup lang="ts">
import { ArrowUpRight, Clock3 } from '@lucide/vue'
import { computed } from 'vue'

import AccountBalanceList from '@/components/common/AccountBalanceList.vue'
import EventCalendarBoard from '@/components/common/EventCalendarBoard.vue'
import MetricCard from '@/components/common/MetricCard.vue'
import PageHeading from '@/components/common/PageHeading.vue'
import Panel from '@/components/common/Panel.vue'
import PositionSyncFeedback from '@/components/common/PositionSyncFeedback.vue'
import ActivePositionList from '@/components/overview/ActivePositionList.vue'
import { t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import { localDateTimeLabel } from '@/utils/date'

const store = useAppStore()

// 合约净仓只展示一次，不按本机订单历史累计义务。
const activePositions = computed(() =>
  [...store.openPositions].sort((left, right) => new Date(left.expiryAt).getTime() - new Date(right.expiryAt).getTime()),
)
</script>

<template>
  <PageHeading :title="t('ui.overview')">
    <div v-if="store.account?.updatedAt" class="snapshot-time">
      <Clock3 :size="15" aria-hidden="true" />
      <span>{{ t('ui.accountUpdated') }}</span>
      <time :datetime="store.account.updatedAt">{{ localDateTimeLabel(store.account.updatedAt) }}</time>
    </div>
  </PageHeading>
  <div class="view-grid overview-grid">
    <div class="metric-strip span-12">
      <MetricCard v-for="metric in store.metrics" :key="metric.id" :metric="metric" />
    </div>

    <Panel :title="t('ui.accountOverview')" class="overview-assets">
      <template #action>
        <button class="panel-link" type="button" :aria-label="t('ui.positionsSpot')" :title="t('ui.positionsSpot')" @click="store.openView('portfolio')">
          <ArrowUpRight :size="17" aria-hidden="true" />
        </button>
      </template>
      <AccountBalanceList :account="store.account" />
    </Panel>

    <Panel :title="t('ui.openShortPositions')" class="overview-positions">
      <template #action>
        <button class="panel-link" type="button" :aria-label="t('ui.positionDetails')" :title="t('ui.positionDetails')" @click="store.openView('portfolio')">
          <ArrowUpRight :size="17" aria-hidden="true" />
        </button>
      </template>
      <PositionSyncFeedback :sync="store.positionSync" />
      <ActivePositionList :positions="activePositions" />
    </Panel>

    <EventCalendarBoard class="span-12" />
  </div>
</template>
