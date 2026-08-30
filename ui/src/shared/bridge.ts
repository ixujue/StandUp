import { invoke } from "@tauri-apps/api/core";

export interface Config {
  reminder_interval_min: number;
  break_duration_min: number;
  idle_threshold_min: number;
  fullscreen_break: boolean;
  /** 全局主题(皮肤):aurora 夜航 / forest 森野 / dawn 晨雾,统管本体与休息页。 */
  break_theme: string;
  autostart: boolean;
  /** 常驻模式(D12):忽略键鼠空闲、始终计时;锁屏/睡眠仍判离开。 */
  resident_mode: boolean;
}

export interface DaySummary {
  seated_min: number;
  reminders: number;
  breaks: number;
  break_min: number;
}

export type State = "Active" | "OnBreak" | "Away" | "Paused";

export interface Snapshot {
  state: State;
  seated_ms: number;
  next_reminder_in_ms: number | null;
  pause_until_ms: number | null;
  card_visible: boolean;
  break_remaining_ms: number | null;
}

export interface Dashboard {
  summary: DaySummary;
  snapshot: Snapshot;
}

/** WebDAV 同步状态(D13;密码只在系统凭证管理器,永不回传前端) */
export interface SyncInfo {
  enabled: boolean;
  url: string;
  username: string;
  has_password: boolean;
  last_sync_at: number;
  last_sync_ok: boolean;
  last_error: string;
}

/** 主题预设(皮肤系统雏形):swatch 用于设置窗的色卡按钮。 */
export const SKINS = [
  { value: "aurora", label: "夜航", swatch: "linear-gradient(135deg, #0b1026, #22306b)" },
  { value: "forest", label: "森野", swatch: "linear-gradient(135deg, #0a1b15, #1f5040)" },
  { value: "dawn", label: "晨雾", swatch: "linear-gradient(135deg, #f7f3ec, #dfe9f2)" },
] as const;

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  dashboard: () => invoke<Dashboard>("get_dashboard"),
  startBreak: () => invoke<void>("start_break"),
  dismissCard: () => invoke<void>("dismiss_card"),
  endBreak: (completed: boolean) => invoke<void>("end_break", { completed }),
  pause: (kind: "1h" | "today") => invoke<void>("pause", { kind }),
  resume: () => invoke<void>("resume"),
  hideMain: () => invoke<void>("hide_main"),
  getSyncInfo: () => invoke<SyncInfo>("get_sync_info"),
  saveSyncSettings: (s: { enabled: boolean; url: string; username: string; password?: string }) =>
    invoke<void>("save_sync_settings", s),
  syncNow: () => invoke<string>("sync_now"),
};
