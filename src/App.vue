<script setup lang="ts">
import { darkTheme, dateZhCN, NConfigProvider, NDialogProvider, NMessageProvider, zhCN } from 'naive-ui'
import type { GlobalThemeOverrides } from 'naive-ui'
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted } from 'vue'
import AppShell from '@/components/layout/AppShell.vue'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

const OverviewView = defineAsyncComponent(() => import('@/components/views/OverviewView.vue'))

const views = {
  overview: OverviewView,
  dual: defineAsyncComponent(() => import('@/components/views/DualInvestmentView.vue')),
  auto: defineAsyncComponent(() => import('@/components/views/AutoInvestView.vue')),
  orders: defineAsyncComponent(() => import('@/components/views/OrdersView.vue')),
  portfolio: defineAsyncComponent(() => import('@/components/views/PortfolioView.vue')),
  analytics: defineAsyncComponent(() => import('@/components/views/AnalyticsView.vue')),
  risk: defineAsyncComponent(() => import('@/components/views/RiskView.vue')),
  settings: defineAsyncComponent(() => import('@/components/views/SettingsView.vue')),
}

const currentView = computed(() => views[store.activeView as keyof typeof views] ?? OverviewView)
let syncTimer: number | undefined

const accentColor = '#f5b625'
const accentColorHover = '#ffcf55'
const accentColorPressed = '#c58e14'
const accentFocusShadow = '0 0 0 2px rgba(245, 182, 37, 0.24)'

const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: accentColor,
    primaryColorHover: accentColorHover,
    primaryColorPressed: accentColorPressed,
    primaryColorSuppl: accentColor,
    bodyColor: '#071017',
    cardColor: '#101922',
    modalColor: '#101922',
    borderColor: 'rgba(255,255,255,0.09)',
    textColorBase: '#f4f7fb',
    textColor1: '#f4f7fb',
    textColor2: '#c5ced9',
    textColor3: '#8d98a6',
  },
  Switch: {
    railColorActive: accentColor,
    loadingColor: accentColor,
    boxShadowFocus: accentFocusShadow,
  },
  Slider: {
    fillColor: accentColor,
    fillColorHover: accentColorHover,
    dotBorderActive: `2px solid ${accentColor}`,
    handleBoxShadowFocus: accentFocusShadow,
  },
  Checkbox: {
    colorChecked: accentColor,
    borderChecked: `1px solid ${accentColor}`,
    borderFocus: `1px solid ${accentColor}`,
    checkMarkColor: '#071017',
    boxShadowFocus: accentFocusShadow,
  },
  Progress: {
    fillColor: accentColor,
    fillColorInfo: accentColor,
    fillColorWarning: accentColorHover,
  },
}

onMounted(() => {
  void store.bootstrap()
  syncTimer = window.setInterval(() => {
    if (store.settings?.connected) {
      void store.syncLiveState()
    }
  }, 60_000)
})

onBeforeUnmount(() => {
  if (syncTimer !== undefined) {
    window.clearInterval(syncTimer)
  }
})
</script>

<template>
  <NConfigProvider :theme="darkTheme" :theme-overrides="themeOverrides" :locale="zhCN" :date-locale="dateZhCN">
    <NDialogProvider>
      <NMessageProvider>
        <AppShell>
          <component :is="currentView" />
        </AppShell>
      </NMessageProvider>
    </NDialogProvider>
  </NConfigProvider>
</template>
