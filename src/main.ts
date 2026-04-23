// Vue 应用入口。
//
// 只负责挂载根组件和 Pinia；Tauri IPC、实盘状态和页面刷新逻辑分别在 service/store 中处理。

import { createPinia } from 'pinia'
import { createApp } from 'vue'
import App from './App.vue'
import './styles.css'

createApp(App)
  .use(createPinia())
  .mount('#app')
