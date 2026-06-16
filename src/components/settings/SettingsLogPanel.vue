<!--
设置页日志面板。

日志只用于本机排障展示，删除日志不会影响订单、恢复记录或 keyring 凭据。所有文件操作
都通过 Rust command 执行，普通浏览器环境不提供 fallback。
-->
<script setup lang="ts">
import { FileText, FolderOpen, RefreshCw, Trash2 } from '@lucide/vue'

import { NAlert, NButton, NIcon, NTag } from 'naive-ui'
import Panel from '@/components/common/Panel.vue'
import { useLogSnapshot } from '@/composables/useLogSnapshot'
import { messageText, t } from '@/i18n'
import { localDateTimeLabel } from '@/utils/date'
import { logLevelTone } from '@/utils/labels'

const logs = useLogSnapshot()

async function clearLogs() {
  if (!window.confirm(t('ui.deleteAllLocalLogFilesOrdersRecovery'))) {
    return
  }
  await logs.clear()
}
</script>

<template>
  <Panel :title="t('ui.logs')" class="settings-logs">
    <template #action>
      <NButton size="small" secondary :loading="logs.pendingAction.value === 'refresh'" @click="logs.refresh">
        <template #icon><NIcon :component="RefreshCw" /></template> {{ t('ui.refresh') }}
      </NButton>
    </template>

    <NAlert v-if="logs.error.value" type="error" class="settings-log-error">
      {{ messageText(logs.error.value) }}
    </NAlert>

    <NAlert v-if="logs.writeError.value" type="warning" class="settings-log-error"> {{ t('ui.localLogWriteFailed') }} {{ messageText(logs.writeError.value) }} {{ t('ui.orderRecordsRemainTheSourceOfTrading') }} </NAlert>

    <div class="settings-log-actions">
      <NButton secondary :loading="logs.pendingAction.value === 'openFile'" @click="logs.openCurrentFile">
        <template #icon><NIcon :component="FileText" /></template> {{ t('ui.openTodaySLog') }}
      </NButton>
      <NButton secondary :loading="logs.pendingAction.value === 'openFolder'" @click="logs.openFolder">
        <template #icon><NIcon :component="FolderOpen" /></template> {{ t('ui.openLogFolder') }}
      </NButton>
      <NButton secondary type="error" :loading="logs.pendingAction.value === 'clear'" :disabled="!logs.hasFiles.value" @click="clearLogs">
        <template #icon><NIcon :component="Trash2" /></template> {{ t('ui.deleteLogs') }}
      </NButton>
    </div>

    <section class="settings-log-section">
      <h3> {{ t('ui.recentLogs') }} </h3>
      <div v-if="logs.recentEntries.value.length === 0" class="settings-empty-row"> {{ t('ui.noLogEntries') }} </div>
      <div v-for="entry in logs.recentEntries.value" :key="entry.id" class="settings-log-entry">
        <time>{{ localDateTimeLabel(entry.timestamp) }}</time>
        <NTag :type="logLevelTone(entry.level)" size="small" class="settings-log-level">
          {{ entry.level }}
        </NTag>
        <span class="settings-log-event">{{ entry.event }}</span>
        <p>{{ messageText(entry.message) }}</p>
      </div>
    </section>
  </Panel>
</template>

<style scoped>
.settings-logs {
  min-width: 0;
}

.settings-log-error {
  margin-bottom: 12px;
}

.settings-log-actions {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
  padding: 12px 0;
  border-bottom: 1px solid var(--line);
}

.settings-log-actions :deep(.n-button) {
  width: 100%;
}

.settings-log-section {
  display: grid;
  gap: 8px;
  padding-top: 14px;
}

.settings-log-section h3 {
  margin: 0;
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}

.settings-empty-row {
  min-height: 42px;
  padding: 10px 0;
  color: var(--muted);
}

.settings-log-entry {
  display: grid;
  grid-template-columns: 150px 64px minmax(220px, 0.55fr) minmax(0, 1.45fr);
  gap: 12px;
  align-items: start;
  min-width: 0;
  padding: 9px 0;
  border-bottom: 1px solid var(--line);
}

.settings-log-entry:last-child {
  border-bottom: 0;
}

.settings-log-entry time {
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}

.settings-log-level {
  justify-self: center;
  min-width: 52px;
}

.settings-log-level :deep(.n-tag__content) {
  display: inline-flex;
  justify-content: center;
  width: 100%;
  text-align: center;
  text-transform: lowercase;
}

.settings-log-event {
  overflow: hidden;
  color: var(--muted);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.settings-log-entry p {
  min-width: 0;
  margin: 0;
  overflow-wrap: anywhere;
}

@media (max-width: 1180px) {
  .settings-log-entry {
    grid-template-columns: 142px 54px minmax(0, 1fr);
  }

  .settings-log-entry p {
    grid-column: 1 / -1;
  }
}
</style>
