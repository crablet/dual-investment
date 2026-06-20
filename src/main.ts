// 挂载 Vue 根组件和 Pinia。

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
