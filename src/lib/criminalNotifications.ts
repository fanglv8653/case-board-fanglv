import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

import {
  claimCriminalReminders,
  markCriminalReminder,
  scanCriminalReminderCandidates,
} from "@/lib/api";
import type { CriminalReminderDelivery } from "@/lib/types";
import { createReminderScanCoordinator } from "@/lib/criminalNotificationCore";

const ENABLED_KEY = "caseboard:criminal-notifications-enabled";
export const CRIMINAL_NOTIFICATION_SETTINGS_EVENT =
  "caseboard:criminal-notification-settings-changed";
const SCAN_INTERVAL_MS = 60_000;

export type CriminalNotificationPermission =
  | "granted"
  | "denied"
  | "error";

function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function isCriminalNotificationEnabled(): boolean {
  return storage()?.getItem(ENABLED_KEY) === "true";
}

function writeEnabled(enabled: boolean) {
  try {
    storage()?.setItem(ENABLED_KEY, enabled ? "true" : "false");
  } catch {
    // localStorage 被系统策略禁用时保持默认关闭；仍发事件让 UI 读取真实状态。
  }
  window.dispatchEvent(new CustomEvent(CRIMINAL_NOTIFICATION_SETTINGS_EVENT));
}

export function disableCriminalNotifications() {
  writeEnabled(false);
}

/** 只能由用户点击开关调用；后台启动和定时扫描绝不主动申请权限。 */
export async function enableCriminalNotifications(): Promise<CriminalNotificationPermission> {
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      const permission = await requestPermission();
      granted = permission === "granted";
    }
    writeEnabled(granted);
    if (granted) {
      void runCriminalReminderScan();
      return "granted";
    }
    return "denied";
  } catch {
    writeEnabled(false);
    return "error";
  }
}

export async function getCriminalNotificationPermission(): Promise<CriminalNotificationPermission> {
  try {
    return (await isPermissionGranted()) ? "granted" : "denied";
  } catch {
    return "error";
  }
}

function reminderBody(delivery: CriminalReminderDelivery): string {
  const due = new Date(delivery.scheduled_for);
  const dueText = Number.isNaN(due.getTime())
    ? delivery.scheduled_for
    : due.toLocaleString("zh-CN", {
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
      });
  return `有一项刑事案件工作任务已到计划时间（${dueText}）。请打开案件看板查看并办理。`;
}

const runCriminalReminderScan = createReminderScanCoordinator({
  isEnabled: isCriminalNotificationEnabled,
  isPermissionGranted,
  scanCandidates: scanCriminalReminderCandidates,
  claimDeliveries: (now) =>
    claimCriminalReminders({ now, channel: "windows", limit: 20 }),
  sendDelivery: (delivery) => {
    sendNotification({
      title: "刑事案件任务提醒",
      body: reminderBody(delivery),
    });
  },
  markDelivery: (deliveryId, sent, errorMessage) =>
    markCriminalReminder({
      delivery_id: deliveryId,
      sent,
      error_message: sent ? null : (errorMessage ?? "系统通知发送失败"),
    }),
  now: () => new Date().toISOString(),
});

let runtimeStarted = false;

export function startCriminalNotificationRuntime() {
  if (runtimeStarted) return;
  runtimeStarted = true;

  const scan = () => {
    void runCriminalReminderScan().catch((error) => {
      console.warn("[criminal-notifications] scan failed", error);
    });
  };

  scan();
  window.addEventListener("focus", scan);
  window.addEventListener(CRIMINAL_NOTIFICATION_SETTINGS_EVENT, scan);
  window.setInterval(scan, SCAN_INTERVAL_MS);
}

/** 用户点击“发送测试提醒”才会请求权限；不会顺带开启自动提醒。 */
export async function sendCriminalNotificationTest(): Promise<CriminalNotificationPermission> {
  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      const permission = await requestPermission();
      granted = permission === "granted";
    }
    if (!granted) return "denied";
    sendNotification({
      title: "方律案件看板测试提醒",
      body: "Windows 系统通知可用。自动提醒仅在应用运行期间发送。",
    });
    return "granted";
  } catch {
    return "error";
  }
}
