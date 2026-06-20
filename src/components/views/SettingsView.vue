<!-- 设置页面：语言、Binance 连接和日志。 -->
<script setup lang="ts">
import PageHeading from '@/components/common/PageHeading.vue'
import BinanceConnectionPanel from '@/components/settings/BinanceConnectionPanel.vue'
import GeneralSettingsPanel from '@/components/settings/GeneralSettingsPanel.vue'
import SettingsLogPanel from '@/components/settings/SettingsLogPanel.vue'
import { t } from '@/i18n'
</script>

<template>
  <PageHeading :title="t('ui.settings')" />
  <div class="settings-page">
    <div class="settings-top">
      <BinanceConnectionPanel />
      <GeneralSettingsPanel />
    </div>
    <SettingsLogPanel />
  </div>
</template>

<style scoped>
.settings-page {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: var(--gap);
  align-items: start;
}

.settings-top {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(260px, 0.34fr);
  gap: var(--gap);
  align-items: start;
}
@container workspace (max-width: 760px) {
  .settings-top {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
