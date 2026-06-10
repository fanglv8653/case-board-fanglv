#!/usr/bin/env python3
"""读 stdin 的 usage_events JSON,输出中文使用情况汇总。

只处理匿名计数/时长,无案件数据。给 summary.sh 调用。
"""
import json
import sys
from collections import defaultdict
from datetime import datetime, timezone, timedelta

# 北京时间
CST = timezone(timedelta(hours=8))


def parse_ts(s: str) -> datetime:
    # PostgREST 返回如 2026-05-31T03:18:11.679407+00:00
    return datetime.fromisoformat(s).astimezone(CST)


def fmt_day(dt: datetime) -> str:
    return dt.strftime("%m/%d")


def main() -> None:
    raw = sys.stdin.read().strip()
    try:
        events = json.loads(raw)
    except json.JSONDecodeError:
        print("✗ 读不到数据(返回不是 JSON):")
        print(raw[:500])
        return

    if isinstance(events, dict) and events.get("message"):
        print(f"✗ Supabase 报错:{events.get('message')}")
        return
    if not events:
        print("📊 还没有任何使用记录。")
        print("   (同事装了带遥测的 dmg 并打开后,这里才会有数据。)")
        return

    # 过滤掉自测探针
    events = [e for e in events if e.get("device_id") not in ("verify-probe", "curl-auth-probe", "probeA", "probeB")]
    if not events:
        print("📊 目前只有测试探针记录,没有真实使用。")
        return

    now = datetime.now(CST)

    # 按设备聚合
    by_dev = defaultdict(list)
    for e in events:
        by_dev[e["device_id"]].append(e)

    # 全局每日活跃设备
    daily_devices = defaultdict(set)
    for e in events:
        d = fmt_day(parse_ts(e["created_at"]))
        daily_devices[d].add(e["device_id"])

    print("=" * 52)
    print(f"📊 CaseBoard 使用情况 · 截至 {now.strftime('%Y-%m-%d %H:%M')}(北京时间)")
    print("=" * 52)
    print(f"\n活跃设备:{len(by_dev)} 台\n")

    # 每台设备一行
    rows = []
    for dev, evs in by_dev.items():
        times = [parse_ts(e["created_at"]) for e in evs]
        first, last = min(times), max(times)
        active_days = len({t.strftime("%Y-%m-%d") for t in times})
        sessions = len({e.get("session_id") for e in evs if e.get("session_id")})
        heartbeats = sum(1 for e in evs if e.get("event_type") == "heartbeat")
        est_min = heartbeats * 5
        ver = max((e.get("app_version") or "?") for e in evs)
        days_since = (now.date() - last.date()).days
        rows.append((last, dev, first, last, active_days, sessions, est_min, ver, days_since))

    # 最近活跃的排前面
    rows.sort(key=lambda r: r[0], reverse=True)

    for i, (_, dev, first, last, active_days, sessions, est_min, ver, days_since) in enumerate(rows, 1):
        short = dev[:8]
        # 留存判断
        if active_days >= 3 and days_since <= 2:
            tag = "✅ 在持续用"
        elif sessions <= 1 or (active_days == 1 and days_since >= 3):
            tag = "⚠️ 试了一下就没再用"
        elif days_since >= 7:
            tag = "💤 超过一周没打开"
        else:
            tag = "· 偶尔用"
        print(f"  {i}. 设备 {short}  v{ver}")
        print(f"     首次 {fmt_day(first)} · 最近 {fmt_day(last)}（{days_since} 天前）"
              f" · 活跃 {active_days} 天 · 开了 {sessions} 次 · 约 {est_min} 分钟  {tag}")

    # 最近 7 天每日活跃
    print("\n最近 7 天每日活跃设备数:")
    line = []
    for i in range(6, -1, -1):
        day = now - timedelta(days=i)
        key = fmt_day(day)
        line.append(f"{key}:{len(daily_devices.get(key, set()))}")
    print("  " + "   ".join(line))

    print("\n说明:时长是下界(每心跳算满 5 分钟),不足 5 分钟的会话算 0;看趋势/留存为主。")


if __name__ == "__main__":
    main()
