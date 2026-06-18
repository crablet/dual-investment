<script setup lang="ts">
import type { PositionSyncStatus } from '@/types/domain'
import { NAlert } from 'naive-ui'
import { messageText, t } from '@/i18n'
import { localDateTimeLabel } from '@/utils/date'

defineProps<{ sync?: PositionSyncStatus }>()
</script>

<template>
  <NAlert v-if="sync?.error" type="warning">
    {{ t('ui.positionSyncFailed') }} {{ messageText(sync.error) }}
    <template v-if="sync.updatedAt">
      {{ t('ui.showingTheLastSuccessfulQuery') }} {{ localDateTimeLabel(sync.updatedAt) }}
    </template>
  </NAlert>
</template>
