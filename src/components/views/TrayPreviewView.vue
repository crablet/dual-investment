<script setup lang="ts">
import { Activity, ClipboardList, Home, PauseCircle, Settings, Zap } from 'lucide-vue-next'
import { NButton, NIcon } from 'naive-ui'
import Panel from '@/components/common/Panel.vue'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
</script>

<template>
  <div class="tray-page">
    <section class="tray-card">
      <header>
        <div class="brand-mark"><NIcon :component="Activity" /></div>
        <strong>后台助手</strong>
        <NButton quaternary circle><NIcon :component="Settings" /></NButton>
      </header>
      <div class="tray-status">
        <p><span class="dot green" />运行状态：<strong>正常</strong></p>
        <p>下一次扫描 <strong>01:23:45</strong></p>
        <p>今日已执行 <strong>11 次</strong></p>
        <p>今日已收权利金 <strong>¥ 186.72</strong></p>
      </div>
      <div class="tray-actions">
        <NButton type="primary" block><template #icon><NIcon :component="Zap" /></template>快速下单</NButton>
        <NButton secondary block @click="store.toggleAuto({ enabled: false, confirmLive: true })"><template #icon><NIcon :component="PauseCircle" /></template>暂停自动定投</NButton>
        <NButton secondary block @click="store.activeView = 'orders'"><template #icon><NIcon :component="ClipboardList" /></template>查看订单</NButton>
        <NButton secondary block @click="store.activeView = 'overview'"><template #icon><NIcon :component="Home" /></template>打开主界面</NButton>
      </div>
    </section>

    <section class="tray-benefits">
      <article><NIcon :component="Activity" /><div><strong>快速查看状态</strong><span>关键运行信息一目了然，随时掌握任务进度。</span></div></article>
      <article><NIcon :component="Zap" /><div><strong>一键执行常用操作</strong><span>常用功能快捷触达，降低操作成本。</span></div></article>
      <article><NIcon :component="Home" /><div><strong>不打断前台工作</strong><span>常驻系统托盘，不占用屏幕空间。</span></div></article>
    </section>

    <Panel title="使用场景">
      <div class="pill-row">
        <span>自动定投运行中</span>
        <span>策略持续监控中</span>
        <span>日常办公 / 学习场景</span>
      </div>
    </Panel>
  </div>
</template>
