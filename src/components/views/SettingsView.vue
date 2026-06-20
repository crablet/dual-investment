<!-- 设置页面：语言、Binance 连接和日志。 -->
<script setup lang="ts">
import BinanceConnectionPanel from '@/components/settings/BinanceConnectionPanel.vue'
import GeneralSettingsPanel from '@/components/settings/GeneralSettingsPanel.vue'
import SettingsLogPanel from '@/components/settings/SettingsLogPanel.vue'
</script>

<template>
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
  gap: 12px;
  align-items: start;
}

.settings-top {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(240px, 280px);
  gap: 12px;
  align-items: start;
}
</style>
