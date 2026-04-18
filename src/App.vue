<script setup lang="ts">
import { darkTheme, NConfigProvider, NDialogProvider, NMessageProvider } from 'naive-ui'
import { computed, defineAsyncComponent, onMounted } from 'vue'
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
  tray: defineAsyncComponent(() => import('@/components/views/TrayPreviewView.vue')),
}

const currentView = computed(() => views[store.activeView as keyof typeof views] ?? OverviewView)

const themeOverrides = {
  common: {
    primaryColor: '#f5b625',
    primaryColorHover: '#ffcf55',
    primaryColorPressed: '#c58e14',
    bodyColor: '#071017',
    cardColor: '#101922',
    modalColor: '#101922',
    borderColor: 'rgba(255,255,255,0.09)',
    textColorBase: '#f4f7fb',
    textColor1: '#f4f7fb',
    textColor2: '#c5ced9',
    textColor3: '#8d98a6',
  },
}

onMounted(() => {
  void store.refresh()
})
</script>

<template>
  <NConfigProvider :theme="darkTheme" :theme-overrides="themeOverrides">
    <NDialogProvider>
      <NMessageProvider>
        <AppShell>
          <component :is="currentView" />
        </AppShell>
      </NMessageProvider>
    </NDialogProvider>
  </NConfigProvider>
</template>
