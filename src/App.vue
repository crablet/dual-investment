<!--
根组件负责全局 UI provider、异步视图路由和后台静默同步。

业务状态全部放在 Pinia store，视图组件只消费当前快照和触发显式操作。
-->
<script setup lang="ts">
import { darkTheme, dateEnUS, dateEo, dateEsAR, dateFrFR, dateIdID, dateJaJP, dateKoKR, datePtBR, dateThTH, dateTrTR, dateViVN, dateZhCN, dateZhTW, enUS, eo, esAR, frFR, idID, jaJP, koKR, NConfigProvider, NMessageProvider, ptBR, thTH, trTR, viVN, zhCN, zhTW } from 'naive-ui'
import { computed, defineAsyncComponent, onMounted, onUnmounted } from 'vue'
import AppShell from '@/components/layout/AppShell.vue'
import { useBackgroundSync } from '@/composables/useBackgroundSync'
import { useAppStore } from '@/stores/app'
import { usePreferencesStore } from '@/stores/preferences'
import { themeOverrides } from '@/theme/naive'

const preferences = usePreferencesStore()
const locales = { 'zh-CN': zhCN, 'zh-TW': zhTW, 'en': enUS, 'pt-BR': ptBR, 'es': esAR, 'vi': viVN, 'tr': trTR, 'id': idID, 'ja': jaJP, 'ko': koKR, 'th': thTH, 'eo': eo, 'fr': frFR }
const dates = { 'zh-CN': dateZhCN, 'zh-TW': dateZhTW, 'en': dateEnUS, 'pt-BR': datePtBR, 'es': dateEsAR, 'vi': dateViVN, 'tr': dateTrTR, 'id': dateIdID, 'ja': dateJaJP, 'ko': dateKoKR, 'th': dateThTH, 'eo': dateEo, 'fr': dateFrFR }
onMounted(() => window.addEventListener('languagechange', preferences.refreshSystemLocale))
onUnmounted(() => window.removeEventListener('languagechange', preferences.refreshSystemLocale))
const store = useAppStore()
useBackgroundSync()

const OverviewView = defineAsyncComponent(() => import('@/components/views/OverviewView.vue'))
const DualInvestmentView = defineAsyncComponent(() => import('@/components/views/DualInvestmentView.vue'))

const views = {
  overview: OverviewView,
  dual: DualInvestmentView,
  orders: defineAsyncComponent(() => import('@/components/views/OrdersView.vue')),
  portfolio: defineAsyncComponent(() => import('@/components/views/PortfolioView.vue')),
  analytics: defineAsyncComponent(() => import('@/components/views/AnalyticsView.vue')),
  settings: defineAsyncComponent(() => import('@/components/views/SettingsView.vue')),
}

const currentView = computed(() => views[store.activeView] ?? OverviewView)
</script>

<template>
  <NConfigProvider :theme="darkTheme" :theme-overrides="themeOverrides" :locale="locales[preferences.locale]" :date-locale="dates[preferences.locale]">
    <NMessageProvider>
      <AppShell>
        <component :is="currentView" />
      </AppShell>
    </NMessageProvider>
  </NConfigProvider>
</template>
