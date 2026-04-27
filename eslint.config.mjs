// ESLint 配置。
//
// 使用 Antfu 预设统一 TypeScript、Vue SFC 和 CSS 格式检查；Rust/Tauri 目录交给
// cargo/clippy/fmt，不纳入前端 lint。

import antfu from '@antfu/eslint-config'

export default antfu({
  typescript: true,
  vue: true,
  formatters: {
    css: true,
  },
  ignores: [
    'dist/**',
    'src-tauri/**',
    'target/**',
    'tsconfig.tsbuildinfo',
  ],
  rules: {
    'no-alert': 'off',
    'vue/singleline-html-element-content-newline': 'off',
  },
})
