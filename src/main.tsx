import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/globals.css";
import { installConsoleTap } from "@/lib/console-tap";
import { applyFontScale } from "@/lib/uiScale";
import { startCriminalNotificationRuntime } from "@/lib/criminalNotifications";
import { startFeishuReadonlyAutoPullRuntime } from "@/lib/feishuAutoPull";

// 2026-05-26 V0.1.11:在 React 启动前装 console.error/warn + window.onerror tap,
// 反馈弹窗打开时一次性把累积的报错回传给 Rust 端写进 MD。
installConsoleTap();

// 2026-06-16:React 启动前应用界面字号缩放,避免默认 16px 先渲染再跳变(闪烁)。
applyFontScale();

// 只在应用运行期间扫描；默认关闭，后台不会主动申请系统通知权限。
startCriminalNotificationRuntime();

// 已连接时自动刷新“在办”案件只读预演；不会写入飞书或覆盖本地案件业务字段。
startFeishuReadonlyAutoPullRuntime();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
