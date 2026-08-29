<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import { onMounted, ref } from "vue";

import { api } from "../shared/bridge";
import { applyTheme } from "../shared/theme";

const interval = ref(45);

onMounted(async () => {
  applyTheme((await api.getConfig().catch(() => null))?.break_theme);
  try {
    await listen<{ interval_min: number }>("card-shown", (e) => {
      interval.value = e.payload.interval_min;
    });
    await listen<{ break_theme: string }>("config-changed", (e) => {
      applyTheme(e.payload.break_theme);
    });
  } catch {
    // 浏览器预览模式没有 Tauri API,忽略
  }
});
</script>

<template>
  <!-- 休息卡片(D7):右下角弹出、透明窗口内的圆角卡;3 分钟未响应由核心自动淡出 -->
  <div class="h-screen p-2">
    <div
      data-tauri-drag-region
      class="skin-card relative flex h-full flex-col justify-between overflow-hidden p-4 shadow-2xl"
    >
      <!-- 品牌渐变顶线 -->
      <div
        data-tauri-drag-region
        class="absolute inset-x-0 top-0 h-1"
        :style="{ background: 'var(--accent-grad)' }"
      />
      <div data-tauri-drag-region>
        <p class="mt-1 text-[15px] font-semibold">已经坐了 {{ interval }} 分钟啦</p>
        <p class="mt-1 text-xs sub">起来活动一下吧,顺便看看远处</p>
      </div>
      <div class="mt-3 flex items-center gap-2">
        <button
          class="flex-1 rounded-xl px-3 py-2 text-sm font-medium text-white shadow-sm transition hover:brightness-110 active:scale-[0.98]"
          :style="{ background: 'var(--accent-grad)' }"
          @click="api.startBreak()"
        >
          开始休息
        </button>
        <button
          class="rounded-xl px-3 py-2 text-sm sub transition hover:bg-white/10"
          @click="api.dismissCard()"
        >
          先不了
        </button>
      </div>
    </div>
  </div>
</template>
