<script setup lang="ts">
// 应用外壳布局。
// 负责侧边导航、前台错误提示和首屏 loading 包裹，页面内容通过 slot 注入。
import { NAlert, NSpin, useMessage } from 'naive-ui'
import { watch } from 'vue'
import SidebarNav from '@/components/layout/SidebarNav.vue'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const message = useMessage()

watch(
  () => store.orderNotice,
  (notice) => {
    if (!notice)
      return
    if (notice.type === 'success') {
      message.success(notice.message)
      return
    }
    message.error(notice.message)
  },
)
</script>

<template>
  <div class="app-shell">
    <SidebarNav />
    <main class="workspace">
      <NAlert v-if="store.error" type="error" closable class="shell-alert">
        {{ store.error }}
      </NAlert>

      <NSpin :show="store.loading && !store.snapshot">
        <section class="view-surface">
          <slot />
        </section>
      </NSpin>
    </main>
  </div>
</template>
