<!--
设置视图。

组合界面偏好、Binance 凭据连接和本地文件日志面板；各面板保持独立状态与操作。
-->
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
