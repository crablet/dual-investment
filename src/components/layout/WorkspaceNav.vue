<script setup lang="ts">
import type { AppViewId } from '@/stores/app'
import { ChartNoAxesColumnIncreasing, ClipboardList, Layers2, LayoutGrid, Radio, Settings, Wallet } from '@lucide/vue'
import { computed } from 'vue'
import { t } from '@/i18n'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const pages = [
  { id: 'overview', label: 'ui.overview', icon: LayoutGrid },
  { id: 'dual', label: 'ui.dualInvestment', icon: Layers2 },
  { id: 'orders', label: 'ui.strategyOrders', icon: ClipboardList },
  { id: 'portfolio', label: 'ui.positionsSpot', icon: Wallet },
  { id: 'analytics', label: 'ui.returns', icon: ChartNoAxesColumnIncreasing },
  { id: 'settings', label: 'ui.settings', icon: Settings },
] as const satisfies ReadonlyArray<{ id: AppViewId, label: string, icon: typeof LayoutGrid }>

const connectionLabel = computed(() => store.settings
  ? t(store.settings.connected ? 'ui.connected' : 'ui.disconnected')
  : '—')
</script>

<template>
  <aside class="workspace-sidebar">
    <div class="sidebar-brand">
      <img src="/favicon.svg" width="38" height="38" alt="">
      <span>{{ t('ui.appTitle') }}</span>
    </div>

    <nav class="sidebar-nav" :aria-label="t('ui.appTitle')">
      <button
        v-for="page in pages"
        :key="page.id"
        class="nav-item"
        :class="{ 'nav-settings': page.id === 'settings' }"
        :aria-current="store.activeView === page.id ? 'page' : undefined"
        :title="t(page.label)"
        type="button"
        @click="store.openView(page.id)"
      >
        <span class="nav-icon" aria-hidden="true"><component :is="page.icon" :size="18" /></span>
        <span class="nav-label">{{ t(page.label) }}</span>
      </button>
    </nav>

    <div class="connection-state" role="status" :aria-label="t('ui.binanceConnectionStatus')" :title="`Binance · ${connectionLabel}`" :class="{ connected: store.settings?.connected }">
      <span class="connection-icon" aria-hidden="true"><Radio :size="19" /></span>
      <div class="connection-label">
        <strong>Binance</strong>
        <span>{{ connectionLabel }}</span>
      </div>
    </div>
  </aside>
</template>

<style scoped>
.workspace-sidebar {
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 24px 14px 18px;
  overflow-y: auto;
  border-inline-end: 1px solid var(--line);
  background: var(--inset);
}
.sidebar-brand {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  margin-inline: 6px;
  padding: 2px 2px 22px;
  border-bottom: 1px solid var(--line);
  font-size: 17px;
  font-weight: 700;
  line-height: 1.35;
}
.sidebar-brand span {
  flex: 1 1 min-content;
  min-width: 0;
  max-width: 100%;
  overflow-wrap: break-word;
}
.sidebar-brand img {
  flex: none;
}
.sidebar-nav {
  display: flex;
  flex: 1 0 auto;
  flex-direction: column;
  gap: 5px;
  padding-block: 18px 12px;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 46px;
  padding: 6px 10px;
  border: 1px solid transparent;
  border-radius: 9px;
  color: #c1c5ca;
  background: transparent;
  text-align: start;
  line-height: 1.35;
  font-weight: 550;
  cursor: pointer;
}
.nav-icon,
.connection-icon {
  display: grid;
  flex: none;
  place-items: center;
  width: 32px;
  height: 32px;
  border-radius: 7px;
  color: var(--muted);
}
.nav-label {
  overflow-wrap: anywhere;
}
.nav-item:hover {
  color: var(--text);
  background: var(--raised);
}
.nav-item[aria-current] {
  border-color: #504632;
  color: var(--gold);
  background: #2d291f;
}
.nav-item[aria-current] .nav-icon {
  color: #28210c;
  background: var(--gold);
}
.nav-settings {
  margin-block-start: auto;
}

.connection-state {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-inline: 6px;
  padding: 16px 4px 0;
  border-top: 1px solid var(--line);
}
.connection-icon {
  background: var(--raised);
}
.connection-label {
  display: grid;
  gap: 2px;
  min-width: 0;
}
.connection-label strong {
  font-size: 14px;
  font-weight: 650;
}
.connection-label span {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--muted);
  font-size: 12px;
  overflow-wrap: anywhere;
}
.connection-label span::before {
  flex: none;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
  content: '';
}
.connected .connection-label span,
.connected .connection-icon {
  color: var(--green);
}

@media (max-width: 1280px) {
  .sidebar-brand {
    gap: 8px;
    font-size: 15px;
  }
  .sidebar-brand img {
    width: 32px;
    height: 32px;
  }
}
@media (max-width: 1000px) {
  .workspace-sidebar {
    padding-inline: 8px;
  }
  .sidebar-brand,
  .connection-state {
    justify-content: center;
    margin-inline: 0;
    padding-inline: 0;
  }
  .nav-item {
    justify-content: center;
    padding-inline: 0;
  }
  .sidebar-brand span,
  .nav-label,
  .connection-label {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
}
@media (prefers-reduced-motion: no-preference) {
  .nav-item,
  .nav-icon {
    transition:
      color 150ms ease,
      background-color 150ms ease,
      border-color 150ms ease;
  }
}
</style>
