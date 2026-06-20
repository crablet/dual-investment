<!-- 页面布局：左侧导航、错误提示、加载状态和内容插槽。 -->
<script setup lang="ts">
import { NAlert, NSpin, useMessage } from 'naive-ui'

import { useTemplateRef, watch } from 'vue'
import WorkspaceNav from '@/components/layout/WorkspaceNav.vue'
import { messageText } from '@/i18n'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const message = useMessage()
const workspace = useTemplateRef<HTMLElement>('workspace')

watch(() => store.activeView, () => workspace.value?.scrollTo(0, 0), { flush: 'post' })

watch(
  () => store.orderNotice,
  (notice) => {
    if (!notice)
      return
    if (notice.type === 'success') {
      message.success(messageText(notice.message))
      return
    }
    message.error(messageText(notice.message))
  },
)
</script>

<template>
  <div class="app-shell">
    <WorkspaceNav />
    <main ref="workspace" class="workspace">
      <div class="workspace-inner">
        <NAlert v-if="store.error" type="error" closable class="shell-alert">
          {{ messageText(store.error) }}
        </NAlert>
        <NSpin :show="store.loading && !store.snapshot">
          <slot />
        </NSpin>
      </div>
    </main>
  </div>
</template>
