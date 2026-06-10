import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/globals.css";
import { installConsoleTap } from "@/lib/console-tap";

// 2026-05-26 V0.1.11:在 React 启动前装 console.error/warn + window.onerror tap,
// 反馈弹窗打开时一次性把累积的报错回传给 Rust 端写进 MD。
installConsoleTap();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
