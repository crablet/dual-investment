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
