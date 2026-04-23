<script setup lang="ts">
// 通用面板容器。
// 只负责标题、可选说明和右上角操作插槽，具体业务内容由调用方组合。
defineProps<{
  title: string
  subtitle?: string
}>()
</script>

<template>
  <section class="panel">
    <header class="panel-header">
      <div>
        <h2>{{ title }}</h2>
        <p v-if="subtitle">{{ subtitle }}</p>
      </div>
      <slot name="action" />
    </header>
    <slot />
  </section>
</template>
