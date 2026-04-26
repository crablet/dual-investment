<script setup lang="ts">
import type { LogFileInfo } from '@/types/domain'
import { FileText, FolderOpen, RefreshCw, Trash2 } from '@lucide/vue'
import { NAlert, NButton, NIcon, NTag } from 'naive-ui'
import { computed } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { useLogSnapshot } from '@/composables/useLogSnapshot'
import { localDateTimeLabel } from '@/utils/date'
import { logLevelTone } from '@/utils/labels'

const logs = useLogSnapshot()

const latestLogLabel = computed(() => {
  const latest = logs.recentEntries.value[0]
  return latest ? localDateTimeLabel(latest.timestamp) : '暂无记录'
})

async function clearLogs() {
  if (!window.confirm('确认删除全部本地日志文件？订单、恢复记录和 keyring 凭据不会被删除。')) {
    return
  }
  await logs.clear()
}

function fileSizeLabel(bytes: number) {
  if (bytes < 1024) {
    return `${bytes} B`
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`
  }
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

function modifiedLabel(file: LogFileInfo) {
  return file.modifiedAt ? localDateTimeLabel(file.modifiedAt) : '暂无修改时间'
}
</script>

<template>
  <Panel title="日志" class="settings-logs">
    <template #action>
      <NButton size="small" secondary :loading="logs.pendingAction.value === 'refresh'" @click="logs.refresh">
        <template #icon><NIcon :component="RefreshCw" /></template>
        刷新
      </NButton>
    </template>

    <NAlert v-if="logs.error.value" type="error" class="settings-log-error">
      {{ logs.error.value }}
    </NAlert>

    <div class="settings-log-summary">
      <div>
        <span>今日日志</span>
        <strong>{{ logs.currentFileName.value }}</strong>
      </div>
      <div>
        <span>日志文件</span>
        <strong>{{ logs.fileCountLabel.value }}</strong>
      </div>
      <div>
        <span>最近记录</span>
        <strong>{{ latestLogLabel }}</strong>
      </div>
    </div>

    <div class="settings-log-path">
      <span>目录</span>
      <strong>{{ logs.snapshot.value?.directory ?? '等待读取' }}</strong>
    </div>

    <div class="settings-log-actions">
      <NButton secondary :loading="logs.pendingAction.value === 'openFile'" @click="logs.openCurrentFile">
        <template #icon><NIcon :component="FileText" /></template>
        打开今日日志
      </NButton>
      <NButton secondary :loading="logs.pendingAction.value === 'openFolder'" @click="logs.openFolder">
        <template #icon><NIcon :component="FolderOpen" /></template>
        打开日志文件夹
      </NButton>
      <NButton secondary type="error" :loading="logs.pendingAction.value === 'clear'" :disabled="logs.files.value.length === 0" @click="clearLogs">
        <template #icon><NIcon :component="Trash2" /></template>
        删除日志
      </NButton>
    </div>

    <section class="settings-log-section">
      <h3>最近日志</h3>
      <div v-if="logs.recentEntries.value.length === 0" class="settings-empty-row">
        暂无日志记录
      </div>
      <div v-for="entry in logs.recentEntries.value" :key="entry.id" class="settings-log-entry">
        <time>{{ localDateTimeLabel(entry.timestamp) }}</time>
        <NTag :type="logLevelTone(entry.level)" size="small" class="settings-log-level">
          {{ entry.level }}
        </NTag>
        <span class="settings-log-event">{{ entry.event }}</span>
        <p>{{ entry.message }}</p>
      </div>
    </section>

    <section class="settings-log-section">
      <h3>日志文件</h3>
      <div v-if="logs.files.value.length === 0" class="settings-empty-row">
        暂无日志文件
      </div>
      <div v-for="file in logs.files.value" :key="file.path" class="settings-log-file">
        <strong>{{ file.date }}</strong>
        <span>{{ file.entryCount }} 条</span>
        <span>{{ fileSizeLabel(file.sizeBytes) }}</span>
        <time>{{ modifiedLabel(file) }}</time>
      </div>
    </section>
  </Panel>
</template>
