<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import { onMounted, ref, watch } from "vue";

import { api, SKINS, type Config, type Dashboard } from "../shared/bridge";
import { applyTheme } from "../shared/theme";

const config = ref<Config | null>(null);
const dashboard = ref<Dashboard | null>(null);
const saved = ref(false);
const dev = import.meta.env.DEV;

// 即改即存:控件 change 事件立即写盘;watch 兜底处理编程性变更(400ms 防抖)。
let armed = false;
let saveTimer: ReturnType<typeof setTimeout> | undefined;

async function saveNow() {
  if (!armed || !config.value) return;
  clearTimeout(saveTimer);
  try {
    await api.saveConfig(config.value);
    saved.value = true;
    setTimeout(() => (saved.value = false), 1500);
  } catch (e) {
    console.error("自动保存失败", e);
  }
}

watch(
  config,
  () => {
    if (!armed) return;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(saveNow, 400);
  },
  { deep: true },
);

onMounted(async () => {
  config.value = await api.getConfig();
  armed = true;
  applyTheme(config.value.break_theme);
  await refresh();
  await listen("state-changed", refresh);
  await listen<{ break_theme: string }>("config-changed", (e) => {
    applyTheme(e.payload.break_theme);
  });
});

async function refresh() {
  dashboard.value = await api.dashboard();
}
</script>

<template>
  <div class="flex h-screen flex-col">
    <!-- 自绘标题栏(D9) -->
    <header data-tauri-drag-region class="flex items-center justify-between px-4 py-2.5">
      <div class="flex items-center gap-2" data-tauri-drag-region>
        <span
          class="grid h-6 w-6 place-items-center rounded-lg text-xs"
          :style="{ background: 'var(--accent-grad)' }"
        >🧍</span>
        <h1 class="text-sm font-semibold">StandUp</h1>
      </div>
      <button
        class="rounded-lg px-2 py-0.5 text-zinc-400 transition hover:bg-white/10 hover:text-zinc-200"
        title="隐藏到托盘"
        @click="api.hideMain()"
      >
        ✕
      </button>
    </header>

    <main v-if="config && dashboard" class="flex flex-1 flex-col gap-2.5 overflow-y-auto px-4 pb-3">
      <!-- 今日概览(D6-B2) -->
      <section class="skin-card relative overflow-hidden p-3.5">
        <div class="absolute inset-y-0 left-0 w-1" :style="{ background: 'var(--accent-grad)' }" />
        <h2 class="text-xs font-medium sub">今日概览</h2>
        <p class="mt-0.5 text-2xl font-light tabular-nums">
          {{ dashboard.summary.seated_min }}<span class="ml-1 text-xs font-normal sub">分钟久坐</span>
        </p>
        <p class="mt-0.5 text-xs sub">
          提醒 {{ dashboard.summary.reminders }} 次 · 休息 {{ dashboard.summary.breaks }} 次(共
          {{ dashboard.summary.break_min }} 分钟)
        </p>
      </section>

      <!-- 主题(皮肤系统雏形):统管本体与休息页 -->
      <section class="skin-card p-3.5">
        <h2 class="text-xs font-medium sub">主题</h2>
        <div class="mt-2 grid grid-cols-3 gap-2">
          <button
            v-for="s in SKINS"
            :key="s.value"
            class="rounded-xl p-1.5 transition"
            :style="config.break_theme === s.value
              ? { background: 'var(--card)', outline: '2px solid var(--accent)', outlineOffset: '-1px' }
              : { background: 'transparent', border: '1px solid var(--card-border)' }"
            @click="config.break_theme = s.value; saveNow()"
          >
            <span class="block h-7 rounded-lg" :style="{ background: s.swatch }" />
            <span class="mt-1 block text-xs">{{ s.label }}</span>
          </button>
        </div>
      </section>

      <!-- 提醒节奏 -->
      <section class="skin-card p-3.5">
        <h2 class="text-xs font-medium sub">提醒节奏</h2>

        <div class="mt-2.5">
          <div class="flex items-center justify-between text-sm">
            <span>久坐间隔</span>
            <span class="sub tabular-nums">{{ config.reminder_interval_min }} 分钟</span>
          </div>
          <input
            v-model.number="config.reminder_interval_min"
            type="range"
            min="15"
            max="120"
            step="5"
            class="mt-1 w-full"
            @change="saveNow"
          />
        </div>

        <div class="mt-2.5">
          <div class="flex items-center justify-between text-sm">
            <span>休息时长</span>
            <span class="sub tabular-nums">{{ config.break_duration_min }} 分钟</span>
          </div>
          <input
            v-model.number="config.break_duration_min"
            type="range"
            min="1"
            max="15"
            class="mt-1 w-full"
            @change="saveNow"
          />
        </div>

        <div class="mt-2.5">
          <div class="flex items-center justify-between text-sm">
            <span>离开多久暂停</span>
            <span class="sub tabular-nums">{{ config.idle_threshold_min }} 分钟</span>
          </div>
          <input
            v-model.number="config.idle_threshold_min"
            type="range"
            min="2"
            max="15"
            class="mt-1 w-full"
            @change="saveNow"
          />
        </div>
      </section>

      <!-- 通用 -->
      <section class="skin-card p-3.5">
        <h2 class="text-xs font-medium sub">通用</h2>
        <div class="mt-2 flex items-center justify-between text-sm">
          <span>全屏遮罩(休息页)</span>
          <input v-model="config.fullscreen_break" type="checkbox" class="h-4 w-4" @change="saveNow" />
        </div>
        <div class="mt-2 flex items-center justify-between text-sm">
          <span>开机自启</span>
          <input v-model="config.autostart" type="checkbox" class="h-4 w-4" @change="saveNow" />
        </div>
      </section>

      <div class="mt-auto flex items-center justify-between pt-1">
        <button
          v-if="dev"
          class="rounded-lg border border-dashed px-2 py-1 text-xs sub transition hover:opacity-80"
          :style="{ borderColor: 'var(--card-border)' }"
          title="开发调试:走与托盘「立即休息」完全相同的核心链路"
          @click="api.startBreak()"
        >
          立即休息(dev)
        </button>
        <span class="ml-auto text-xs sub">
          <span v-if="saved" style="color: var(--accent)">已保存 ✓ </span>
          改动自动保存
        </span>
      </div>
    </main>
  </div>
</template>
