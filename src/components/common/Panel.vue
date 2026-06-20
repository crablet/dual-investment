<!-- 面板容器，提供标题、操作区和内容插槽。 -->
<script setup lang="ts">
defineProps<{
  title: string
}>()
</script>

<template>
  <section class="panel">
    <header class="panel-header">
      <div>
        <h2>{{ title }}</h2>
      </div>
      <slot name="action" />
    </header>
    <slot />
  </section>
</template>
