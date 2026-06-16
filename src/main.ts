// Vue 应用入口。
//
// 只负责挂载根组件和 Pinia；Tauri IPC、实盘状态和页面刷新逻辑分别在 service/store 中处理。

import { createPinia } from 'pinia'
import { createApp } from 'vue'
import App from './App.vue'
import { i18n } from './i18n'
import { usePreferencesStore } from './stores/preferences'
import './styles.css'

async function bootstrap() {
  const pinia = createPinia()
  await usePreferencesStore(pinia).initializeLocale()

  createApp(App)
    .use(pinia)
    .use(i18n)
    .mount('#app')
}

void bootstrap()
