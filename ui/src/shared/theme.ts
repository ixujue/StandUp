// 全局主题(皮肤):设置 html[data-theme],三个窗口共用同一套 design tokens。

export function applyTheme(theme: string | null | undefined) {
  document.documentElement.setAttribute("data-theme", theme ?? "aurora");
}
