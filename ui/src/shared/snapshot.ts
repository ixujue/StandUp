/** 快照订阅:后端 1 秒心跳广播权威快照,前端按 ts_ms 本地插值,平滑走字。
 *
 * 判定仍以 core 为准(后端事件驱动),这里只做显示层的外推:
 * 两次心跳之间不再有 IPC,页面隐藏时定时器自动停摆。
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { computed, onBeforeUnmount, onMounted, ref, type ComputedRef } from "vue";

import type { Snapshot, SnapshotTick } from "./bridge";

/** 需要走字的状态:其余状态(Away)数值不随时间变化,不开定时器。 */
function ticking(s: Snapshot | null): boolean {
  return s != null && (s.state === "Active" || s.state === "OnBreak" || s.state === "Paused");
}

export interface SnapshotStore {
  /** 插值后的快照(读秒用);后端权威值到达时重置基准。 */
  readonly view: ComputedRef<Snapshot | null>;
  /** 后端最近一次权威快照(未插值)。 */
  readonly raw: ComputedRef<Snapshot | null>;
}

export function useSnapshot(refreshMs = 250): SnapshotStore {
  const snap = ref<Snapshot | null>(null);
  const tsMs = ref(0);
  const nowMs = ref(0);
  let timer: ReturnType<typeof setInterval> | undefined;
  let unlisten: UnlistenFn[] = [];

  function accept(payload: SnapshotTick) {
    snap.value = payload.snapshot;
    tsMs.value = payload.ts_ms;
    nowMs.value = Date.now();
    syncTimer();
  }

  /** 只在需要走字(且页面可见)时跑定时器,其余时间零开销。 */
  function syncTimer() {
    const need = ticking(snap.value) && !document.hidden;
    if (need && !timer) {
      timer = setInterval(() => (nowMs.value = Date.now()), refreshMs);
    } else if (!need && timer) {
      clearInterval(timer);
      timer = undefined;
    }
  }

  function onVisibility() {
    syncTimer();
  }

  const view = computed<Snapshot | null>(() => {
    const s = snap.value;
    if (!s) return null;
    if (!ticking(s)) return s;
    // 心跳基准 + 本地流逝;时钟回拨/长时间挂起时钳到 0,不出现负数
    const d = Math.max(0, Math.min(nowMs.value - tsMs.value, 65_000));
    return {
      ...s,
      seated_ms: s.state === "Active" ? s.seated_ms + d : s.seated_ms,
      next_reminder_in_ms:
        s.next_reminder_in_ms == null ? null : Math.max(0, s.next_reminder_in_ms - d),
      break_remaining_ms:
        s.break_remaining_ms == null ? null : Math.max(0, s.break_remaining_ms - d),
      pause_until_ms: s.pause_until_ms,
    };
  });

  onMounted(async () => {
    try {
      unlisten = [
        await listen<SnapshotTick>("snapshot", (e) => accept(e.payload)),
        await listen<SnapshotTick>("state-changed", (e) => accept(e.payload)),
      ];
      document.addEventListener("visibilitychange", onVisibility);
    } catch {
      // 浏览器直开预览无 Tauri API,忽略
    }
  });

  onBeforeUnmount(() => {
    unlisten.forEach((fn) => fn());
    unlisten = [];
    document.removeEventListener("visibilitychange", onVisibility);
    if (timer) clearInterval(timer);
  });

  return { view, raw: computed(() => snap.value) };
}

/** mm:ss 文本;不足一秒按 1 秒显示,避免出现 00:00 的停顿感。 */
export function mmss(ms: number | null | undefined): string {
  if (ms == null) return "--:--";
  const total = Math.max(0, Math.ceil(ms / 1000));
  return `${String(Math.floor(total / 60)).padStart(2, "0")}:${String(total % 60).padStart(2, "0")}`;
}
