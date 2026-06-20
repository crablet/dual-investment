<!-- 页面布局：侧边导航、错误提示、加载状态和内容插槽。 -->
<script setup lang="ts">
import { NAlert, NSpin, useMessage } from 'naive-ui'

import { watch } from 'vue'
import SidebarNav from '@/components/layout/SidebarNav.vue'
import { messageText } from '@/i18n'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const message = useMessage()

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
    <SidebarNav />
    <main class="workspace">
      <NAlert v-if="store.error" type="error" closable class="shell-alert">
        {{ messageText(store.error) }}
      </NAlert>

      <NSpin :show="store.loading && !store.snapshot">
        <section class="view-surface">
          <slot />
        </section>
      </NSpin>
    </main>
  </div>
</template>
