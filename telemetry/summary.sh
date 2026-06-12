#!/bin/bash
# CaseBoard 遥测一键汇总 —— 用 secret key 读后台,输出中文使用情况报告。
#
# 用法:  bash telemetry/summary.sh
# 依赖:  telemetry/.env.telemetry 里的 CASEBOARD_TELEMETRY_SECRET(本地 gitignored)
#        python3(macOS 自带)
#
# 隐私:secret key 只在本机用,绝不进 git/dmg。输出只有匿名设备ID + 计数/时长。

set -e
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

if [ ! -f telemetry/.env.telemetry ]; then
  echo "✗ 找不到 telemetry/.env.telemetry"; exit 1
fi
set -a; . telemetry/.env.telemetry; set +a

if [ -z "$CASEBOARD_TELEMETRY_SECRET" ]; then
  echo "✗ .env.telemetry 里没有 CASEBOARD_TELEMETRY_SECRET(读后台需要 secret key)"; exit 1
fi

# 拉全部事件(Range 翻页拉全量。曾因 PostgREST 默认单次 1000 行上限只看到最老
# 1000 条,报告把最新数据全截掉了 —— 2026-06-10 修复,别再退回单次 curl)
RAW=$(python3 - <<'PYEOF'
import json, os, urllib.request
url = os.environ["CASEBOARD_TELEMETRY_URL"]
key = os.environ["CASEBOARD_TELEMETRY_SECRET"]
rows, page = [], 0
while True:
    req = urllib.request.Request(
        url + "/rest/v1/usage_events?select=*&order=created_at.asc",
        headers={"apikey": key, "Authorization": "Bearer " + key,
                 "Range-Unit": "items",
                 "Range": f"{page*1000}-{page*1000+999}"})
    batch = json.load(urllib.request.urlopen(req, timeout=30))
    rows += batch
    if len(batch) < 1000:
        break
    page += 1
print(json.dumps(rows))
PYEOF
)

echo "$RAW" | python3 telemetry/summarize.py
