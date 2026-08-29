<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import { computed, onBeforeUnmount, onMounted, ref, watchEffect } from "vue";

import { api } from "../shared/bridge";
import { applyTheme } from "../shared/theme";

const remaining = ref(0);
const planned = ref(0);
const adviceIndex = ref(0);

// 倒计时由前端流畅渲染;判定以核心的 BreakEnd / CloseBreak 为准(设计文档第四节)。
let ticker: ReturnType<typeof setInterval> | undefined;
let adviceTimer: ReturnType<typeof setInterval> | undefined;

const advices = [
  "看看 6 米外的地方",
  "站起来伸展一下肩颈",
  "喝口水,慢慢咽",
  "深呼吸几次,放空一下",
];

// 前 30 秒只有倒计时,之后才出现"提前结束"(D7-S3)
const canEndEarly = computed(() => planned.value - remaining.value >= 30_000);

const RADIUS = 150;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;
const progress = computed(() =>
  planned.value > 0 ? Math.max(0, remaining.value) / planned.value : 0,
);
const dashOffset = computed(() => CIRCUMFERENCE * (1 - progress.value));

const mmss = computed(() => {
  const total = Math.max(0, Math.ceil(remaining.value / 1000));
  return `${String(Math.floor(total / 60)).padStart(2, "0")}:${String(total % 60).padStart(2, "0")}`;
});

onMounted(async () => {
  // 支持浏览器直开预览:http://localhost:5173/break.html?theme=forest
  const urlTheme = new URLSearchParams(location.search).get("theme");
  if (urlTheme) applyTheme(urlTheme);

  try {
    const config = await api.getConfig().catch(() => null);
    if (!urlTheme) applyTheme(config?.break_theme);

    await listen<{ remaining_ms: number | null; theme?: string }>("break-started", (e) => {
      if (e.payload.theme) applyTheme(e.payload.theme);
      remaining.value = e.payload.remaining_ms ?? 0;
      planned.value = remaining.value;
      clearInterval(ticker);
      ticker = setInterval(() => {
        if (remaining.value > 0) remaining.value -= 250;
      }, 250);
    });

    // 设置窗切换主题时实时换肤
    await listen<{ break_theme: string }>("config-changed", (e) => {
      applyTheme(e.payload.break_theme);
    });
  } catch (e) {
    // 浏览器预览模式没有 Tauri API,忽略
    console.warn("tauri api unavailable", e);
  }

  // 窗口标题带上当前主题,便于自动化验证
  watchEffect(() => {
    document.title = `StandUp 休息 [${document.documentElement.getAttribute("data-theme") ?? "aurora"}]`;
  });

  adviceTimer = setInterval(() => {
    adviceIndex.value = (adviceIndex.value + 1) % advices.length;
  }, 30_000);
});

onBeforeUnmount(() => {
  clearInterval(ticker);
  clearInterval(adviceTimer);
});
</script>

<template>
  <div class="flex h-screen w-screen flex-col items-center justify-center gap-2 overflow-hidden">
    <!-- 呼吸光晕 -->
    <div class="pointer-events-none absolute" aria-hidden="true">
      <div
        class="h-[26rem] w-[26rem] rounded-full blur-3xl breathe"
        :style="{ background: 'var(--glow)' }"
      />
    </div>

    <p class="sub z-10 text-sm tracking-[0.3em] uppercase">休息一下,就现在</p>

    <!-- 细线进度环 + 超细大数字 -->
    <div class="relative z-10 my-6 grid place-items-center">
      <svg viewBox="0 0 320 320" class="h-80 w-80 -rotate-90">
        <circle
          cx="160"
          cy="160"
          :r="RADIUS"
          fill="none"
          stroke="var(--ring)"
          stroke-opacity="0.16"
          stroke-width="5"
        />
        <circle
          cx="160"
          cy="160"
          :r="RADIUS"
          fill="none"
          stroke="var(--ring)"
          stroke-width="5"
          stroke-linecap="round"
          :stroke-dasharray="CIRCUMFERENCE"
          :stroke-dashoffset="dashOffset"
          style="transition: stroke-dashoffset 250ms linear"
        />
      </svg>
      <p class="absolute text-8xl font-extralight tabular-nums tracking-tight" style="font-weight: 200">
        {{ mmss }}
      </p>
    </div>

    <transition name="fade" mode="out-in">
      <p :key="adviceIndex" class="sub z-10 text-lg">{{ advices[adviceIndex] }}</p>
    </transition>

    <button
      v-if="canEndEarly"
      class="z-10 mt-10 rounded-full border px-5 py-2 text-sm sub backdrop-blur transition hover:opacity-80"
      :style="{ borderColor: 'var(--card-border)' }"
      @click="api.endBreak(false)"
    >
      提前结束
    </button>
  </div>
</template>

<style scoped>
.breathe {
  animation: breathe 8s ease-in-out infinite;
}
@keyframes breathe {
  0%,
  100% {
    transform: scale(1);
    opacity: 0.55;
  }
  50% {
    transform: scale(1.08);
    opacity: 0.85;
  }
}
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.6s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
