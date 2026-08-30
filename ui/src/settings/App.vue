<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import { computed, onMounted, ref, watch } from "vue";

import { api, SKINS, type Config, type Dashboard, type SyncInfo } from "../shared/bridge";
import { applyTheme } from "../shared/theme";

const config = ref<Config | null>(null);
const dashboard = ref<Dashboard | null>(null);
const saved = ref(false);
const dev = import.meta.env.DEV;

type Tab = "overview" | "reminder" | "appearance" | "sync";
const tab = ref<Tab>("overview");
const TABS: { id: Tab; label: string }[] = [
  { id: "overview", label: "概览" },
  { id: "reminder", label: "提醒" },
  { id: "appearance", label: "外观" },
  { id: "sync", label: "同步" },
];

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

// ---- 同步设置(D13) ----
const sync = ref<SyncInfo | null>(null);
const syncForm = ref({ enabled: false, url: "", username: "" });
const syncPassword = ref("");
const syncing = ref(false);
let syncArmed = false;
let syncTimer: ReturnType<typeof setTimeout> | undefined;

async function refreshSync() {
  sync.value = await api.getSyncInfo();
  if (!syncArmed) {
    syncForm.value = {
      enabled: sync.value.enabled,
      url: sync.value.url,
      username: sync.value.username,
    };
    syncArmed = true;
  }
}

async function saveSync(withPassword = false) {
  if (!syncArmed) return;
  clearTimeout(syncTimer);
  try {
    await api.saveSyncSettings({
      enabled: syncForm.value.enabled,
      url: syncForm.value.url,
      username: syncForm.value.username,
      ...(withPassword && syncPassword.value ? { password: syncPassword.value } : {}),
    });
    saved.value = true;
    setTimeout(() => (saved.value = false), 1500);
    await refreshSync();
  } catch (e) {
    console.error("同步设置保存失败", e);
  }
}

watch(
  syncForm,
  () => {
    if (!syncArmed) return;
    clearTimeout(syncTimer);
    syncTimer = setTimeout(() => saveSync(false), 500);
  },
  { deep: true },
);

async function doSyncNow() {
  syncing.value = true;
  try {
    await api.syncNow();
  } catch (e) {
    console.error("同步失败", e);
  } finally {
    syncing.value = false;
    await refreshSync();
  }
}

const lastSyncText = computed(() => {
  if (!sync.value || !sync.value.last_sync_at) return "尚未同步";
  const time = new Date(sync.value.last_sync_at).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
  });
  return sync.value.last_sync_ok ? `上次 ${time} 同步成功` : `上次 ${time} 失败:${sync.value.last_error}`;
});

const stateText = computed(() => {
  const s = dashboard.value?.snapshot;
  if (!s) return "";
  switch (s.state) {
    case "Active":
      return s.next_reminder_in_ms != null
        ? `计时中 · ${Math.max(1, Math.ceil(s.next_reminder_in_ms / 60000))} 分钟后提醒`
        : "计时中";
    case "OnBreak":
      return `休息中 · 剩余 ${Math.ceil((s.break_remaining_ms ?? 0) / 1000)} 秒`;
    case "Away":
      return "已离开(键鼠空闲)";
    case "Paused":
      return "已暂停";
  }
});

onMounted(async () => {
  config.value = await api.getConfig();
  armed = true;
  applyTheme(config.value.break_theme);
  await refresh();
  await refreshSync();
  await listen("state-changed", refresh);
  await listen("sync-changed", refreshSync);
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
    <!-- 自绘标题栏(D9):整条头部可拖动(✕ 按钮除外),双击最大化/还原 -->
    <header data-tauri-drag-region class="flex items-center justify-between px-4 py-2.5">
      <div class="flex items-center gap-2" data-tauri-drag-region>
        <span data-tauri-drag-region class="grid h-6 w-6 place-items-center rounded-lg text-xs" :style="{ background: 'var(--accent-grad)' }">🧍</span>
        <h1 data-tauri-drag-region class="text-sm font-semibold">StandUp</h1>
      </div>
      <button
        class="rounded-lg px-2 py-0.5 text-zinc-400 transition hover:bg-white/10 hover:text-zinc-200"
        title="隐藏到托盘"
        @click="api.hideMain()"
      >
        ✕
      </button>
    </header>

    <!-- 分页导航 -->
    <nav class="mx-4 mb-1 grid grid-cols-4 gap-1 rounded-xl p-1" :style="{ background: 'var(--track)' }">
      <button
        v-for="t in TABS"
        :key="t.id"
        class="rounded-lg py-1.5 text-xs font-medium transition"
        :style="tab === t.id
          ? { background: 'var(--card)', color: 'var(--text)', outline: '1px solid var(--card-border)' }
          : { color: 'var(--text-sub)' }"
        @click="tab = t.id"
      >
        {{ t.label }}
      </button>
    </nav>

    <main v-if="config && dashboard" class="flex flex-1 flex-col gap-2.5 overflow-y-auto px-4 pb-3">
      <!-- ============ 概览 ============ -->
      <template v-if="tab === 'overview'">
        <section class="skin-card relative overflow-hidden p-3.5">
          <div class="absolute inset-y-0 left-0 w-1" :style="{ background: 'var(--accent-grad)' }" />
          <h2 class="text-xs font-medium sub">今日久坐</h2>
          <p class="mt-0.5 text-3xl font-light tabular-nums">
            {{ dashboard.summary.seated_min }}<span class="ml-1 text-xs font-normal sub">分钟</span>
          </p>
          <p class="mt-0.5 text-xs sub">
            提醒 {{ dashboard.summary.reminders }} 次 · 休息 {{ dashboard.summary.breaks }} 次(共
            {{ dashboard.summary.break_min }} 分钟)
          </p>
        </section>

        <section class="skin-card p-3.5">
          <div class="flex items-center justify-between text-sm">
            <span class="sub">当前状态</span>
            <span>{{ stateText }}</span>
          </div>
          <div class="mt-2.5 grid grid-cols-2 gap-2">
            <button
              class="rounded-xl py-2 text-sm font-medium transition hover:opacity-90"
              :style="{ background: 'var(--accent-grad)' }"
              @click="api.startBreak()"
            >
              立即休息
            </button>
            <button
              v-if="dashboard.snapshot.state === 'Paused'"
              class="rounded-xl py-2 text-sm transition hover:opacity-90"
              :style="{ background: 'var(--track)' }"
              @click="api.resume(); refresh()"
            >
              恢复计时
            </button>
            <button
              v-else
              class="rounded-xl py-2 text-sm transition hover:opacity-90"
              :style="{ background: 'var(--track)' }"
              @click="api.pause('1h'); refresh()"
            >
              暂停 1 小时
            </button>
          </div>
        </section>

        <section class="skin-card p-3.5">
          <div class="flex items-center justify-between text-sm">
            <span>开机自启</span>
            <input v-model="config.autostart" type="checkbox" class="h-4 w-4" @change="saveNow" />
          </div>
        </section>
      </template>

      <!-- ============ 提醒 ============ -->
      <template v-else-if="tab === 'reminder'">
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
            <p class="mt-1 text-xs sub">离开超过该时长视为休息,久坐计时清零重计。</p>
          </div>
        </section>

        <section class="skin-card p-3.5">
          <div class="flex items-center justify-between text-sm">
            <span>常驻模式</span>
            <input v-model="config.resident_mode" type="checkbox" class="h-4 w-4" @change="saveNow" />
          </div>
          <p class="mt-1 text-xs sub">
            {{ config.resident_mode
              ? "已开启:不看键鼠空闲、始终计时;锁屏或关机仍会判为离开。"
              : "开启后不看键鼠空闲、始终计时,适合长时间阅读/看视频;锁屏仍判离开。" }}
          </p>
        </section>
      </template>

      <!-- ============ 外观 ============ -->
      <template v-else-if="tab === 'appearance'">
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

        <section class="skin-card p-3.5">
          <div class="flex items-center justify-between text-sm">
            <span>休息页全屏遮罩</span>
            <input v-model="config.fullscreen_break" type="checkbox" class="h-4 w-4" @change="saveNow" />
          </div>
          <p class="mt-1 text-xs sub">开启后休息页覆盖全屏;关闭则显示为右下角小窗。</p>
        </section>
      </template>

      <!-- ============ 同步 ============ -->
      <template v-else>
        <section class="skin-card p-3.5">
          <div class="flex items-center justify-between text-sm">
            <span>WebDAV 同步</span>
            <input v-model="syncForm.enabled" type="checkbox" class="h-4 w-4" @change="saveSync(false)" />
          </div>
          <p class="mt-1 text-xs sub">
            数据存到你自己的网盘(坚果云/Nextcloud 等),配置以最新修改为准,统计两端合并。
          </p>

          <div class="mt-2.5">
            <label class="text-xs sub">服务器地址</label>
            <input
              v-model="syncForm.url"
              type="url"
              placeholder="https://dav.jianguoyun.com/dav/"
              class="mt-1 w-full rounded-lg px-2.5 py-1.5 text-sm outline-none"
              :style="{ background: 'var(--track)', border: '1px solid var(--card-border)' }"
              @change="saveSync(false)"
            />
          </div>
          <div class="mt-2">
            <label class="text-xs sub">账号</label>
            <input
              v-model="syncForm.username"
              type="text"
              autocomplete="off"
              class="mt-1 w-full rounded-lg px-2.5 py-1.5 text-sm outline-none"
              :style="{ background: 'var(--track)', border: '1px solid var(--card-border)' }"
              @change="saveSync(false)"
            />
          </div>
          <div class="mt-2">
            <label class="text-xs sub">应用密码</label>
            <input
              v-model="syncPassword"
              type="password"
              autocomplete="new-password"
              :placeholder="sync?.has_password ? '已保存(输入新值可更新)' : '坚果云请在网页端生成「应用密码」'"
              class="mt-1 w-full rounded-lg px-2.5 py-1.5 text-sm outline-none"
              :style="{ background: 'var(--track)', border: '1px solid var(--card-border)' }"
              @change="saveSync(true)"
            />
          </div>

          <button
            class="mt-3 w-full rounded-xl py-2 text-sm font-medium transition hover:opacity-90 disabled:opacity-50"
            :style="{ background: 'var(--accent-grad)' }"
            :disabled="syncing || !syncForm.enabled"
            @click="doSyncNow"
          >
            {{ syncing ? "同步中…" : "立即同步" }}
          </button>
          <p class="mt-1.5 text-center text-xs" :style="{ color: sync?.last_sync_ok === false ? 'var(--accent)' : 'var(--text-sub)' }">
            {{ lastSyncText }}
          </p>
        </section>
      </template>

      <div data-tauri-drag-region class="mt-auto flex items-center justify-between pt-1">
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
