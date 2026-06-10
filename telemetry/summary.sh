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

# 拉全部事件(分页上限 PostgREST 默认 1000 行,够用很久;真超了再加 Range 翻页)
RAW=$(curl -s "$CASEBOARD_TELEMETRY_URL/rest/v1/usage_events?select=*&order=created_at.asc" \
  -H "apikey: $CASEBOARD_TELEMETRY_SECRET" \
  -H "Authorization: Bearer $CASEBOARD_TELEMETRY_SECRET")

echo "$RAW" | python3 telemetry/summarize.py
