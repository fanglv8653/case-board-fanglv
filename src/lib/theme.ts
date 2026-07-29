import { useCallback, useEffect, useState } from "react";

export const THEME_STORAGE_KEY = "caseboard-theme";
export const THEME_CHANGE_EVENT = "caseboard-theme-change";

export const THEMES = [
  {
    id: "default",
    label: "方律默认",
    description: "保留当前中性灰与克制状态色，不改变既有界面。",
    swatches: ["#fafafa", "#262626", "#e5e5e5"],
  },
  {
    id: "emerald_ivory",
    label: "墨绿象牙",
    description: "墨绿强调色搭配暖象牙底色，状态色含义保持不变。",
    swatches: ["#f7f3e8", "#175c46", "#dcebdd"],
  },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];

export function isThemeId(value: string | null): value is ThemeId {
  return THEMES.some((theme) => theme.id === value);
}

export function getThemePreference(): ThemeId {
  if (typeof window === "undefined") return "default";
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    return isThemeId(stored) ? stored : "default";
  } catch {
    return "default";
  }
}

export function applyThemePreference(theme = getThemePreference()): ThemeId {
  if (typeof document === "undefined") return theme;
  if (theme === "default") {
    document.documentElement.removeAttribute("data-theme");
  } else {
    document.documentElement.dataset.theme = theme;
  }
  return theme;
}

export function setThemePreference(theme: ThemeId): void {
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // 主题只是本机界面偏好；存储不可用时仍允许当前窗口即时切换。
  }
  applyThemePreference(theme);
  try {
    window.dispatchEvent(new CustomEvent(THEME_CHANGE_EVENT, { detail: theme }));
  } catch {
    // ignore event dispatch failures
  }
}

export function useThemePreference(): [ThemeId, (theme: ThemeId) => void] {
  const [theme, setTheme] = useState<ThemeId>(getThemePreference);

  useEffect(() => {
    const onThemeChange = (event: Event) => {
      const next = (event as CustomEvent<unknown>).detail;
      if (typeof next === "string" && isThemeId(next)) setTheme(next);
    };
    const onStorage = (event: StorageEvent) => {
      if (event.key === THEME_STORAGE_KEY) setTheme(getThemePreference());
    };
    window.addEventListener(THEME_CHANGE_EVENT, onThemeChange);
    window.addEventListener("storage", onStorage);
    return () => {
      window.removeEventListener(THEME_CHANGE_EVENT, onThemeChange);
      window.removeEventListener("storage", onStorage);
    };
  }, []);

  const update = useCallback((next: ThemeId) => {
    setThemePreference(next);
    setTheme(next);
  }, []);

  return [theme, update];
}
