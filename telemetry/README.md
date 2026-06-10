# CaseBoard 匿名使用遥测

**一句话**:看「同事有没有在用 / 用了多久 / 用了之后会不会持续回来(留存)」,**绝不回传任何案件数据**。

## 上报了什么(全部非敏感)

| 字段 | 例 | 说明 |
|:---|:---|:---|
| `device_id` | UUID | 复用 app 的匿名 `client_id`,跟姓名/邮箱无关。按设备区分(不记名) |
| `app_version` | `0.2.3` | 版本 |
| `os` | `macos aarch64` | 粗粒度,**不带系统版本号** |
| `event_type` | `session_start` / `heartbeat` | v1 只有这两种 |
| `session_id` | UUID | 一次开机的随机ID,数会话/去重用 |

**没有**案件名、当事人、文件名、文本、API key。

## 怎么算时长/留存

- 启动立即发 `session_start`,之后每 5 分钟发一次 `heartbeat`(不靠"退出"事件——app 关闭太快发不出去)。
- 后台数 heartbeat ≈ 估时长;`active_days`(活跃天数)是留存核心指标。
- 时长是**粗代理**,看趋势足够,别当精确工时。

## 一次性设置(已做过就跳过)

1. **建表**:Supabase 控制台 → SQL Editor → 粘贴 `supabase_schema.sql` 整段 → Run。
   建了 `usage_events` 表 + RLS(匿名 key 只能写不能读)+ 两个看板视图。
2. **key 注入**:`telemetry/.env.telemetry`(本目录,**gitignored**)存 Supabase URL + key。
   实测(2026-05-30)新版 `sb_publishable_...` key 对 `/rest/v1/usage_events` 鉴权**通过**
   (建表前返回 404 PGRST205「表不存在」= 鉴权 OK,只是表还没建)。代码对 key 格式无感。
   `scripts/release.sh` 出 dmg 时自动 source 它,编译期注入进二进制。
   - dev 模式(`pnpm tauri dev`)不注入 → **开发期不上报**,不污染数据。
   - 缺这个文件出的 dmg → 遥测自动禁用,不报错。

## 端到端验证(作者建表后做,缺一不可)

建表前 curl 返回 404 PGRST205 只证明「key 鉴权通过 + 路由到 PostgREST」,**不证明能真正写入**。
建表后必须实测这两条(成功标准是状态码,不是「表建好了」):

```bash
set -a; . telemetry/.env.telemetry; set +a
# A. 写:必须 201（证明 anon 角色过了 RLS with check）
curl -s -w "\n写=%{http_code}\n" -X POST "$CASEBOARD_TELEMETRY_URL/rest/v1/usage_events" \
  -H "apikey: $CASEBOARD_TELEMETRY_KEY" -H "Authorization: Bearer $CASEBOARD_TELEMETRY_KEY" \
  -H "Content-Type: application/json" -H "Prefer: return=minimal" \
  -d '{"device_id":"verify-probe","event_type":"session_start","session_id":"v1"}'
# B. 读:必须返回 []（证明匿名 key 读不到数据，RLS write-only 生效）
curl -s -w "\n读=%{http_code}\n" "$CASEBOARD_TELEMETRY_URL/rest/v1/usage_events?select=*" \
  -H "apikey: $CASEBOARD_TELEMETRY_KEY" -H "Authorization: Bearer $CASEBOARD_TELEMETRY_KEY"
```
A 不是 201 → key 角色没被 RLS 放行(可能要换 legacy anon JWT)。B 不是 `[]` → RLS 读没锁住,要查 policy。

## 已知边界

- **短会话 `est_minutes_lower_bound`=0**:开了又在 5 分钟内关，只有 1 条 session_start、0 个 heartbeat → 估时长 0，但 `sessions` 仍计 1（按 session_id）。看「有没有用/留存」不受影响，但别把 0 分钟当没用。该字段是**时长下界**(每心跳算满 5 分钟)。
- **device_id 可能分裂**:用户在设置里清空/重生成 `client_id`（换匿名ID），该设备会变成两行，留存曲线断开。少见，可接受。

## 怎么看后台

登录 https://supabase.com → 进项目 → 左侧 **Table Editor**:

- 看 **`device_summary`** 视图:每台设备一行 —— `active_days`(活跃天数,留存核心)多 + `last_seen` 在最近 = 这台在持续用;装了就没再开的设备一眼可见。`est_minutes_lower_bound` = 估时长下界。
- 看 **`daily_active`** 视图:每天还有几台设备在用(整体趋势涨/跌)。

> ⚠️ 你们人少,虽然不记名,但按设备一行、能大致对上是谁。这是看留存必需的粒度。

## 加「用了哪个功能」(v2,按需)

v1 只做 session/heartbeat,已足够回答留存。若以后想看「主要用 chat 还是抽取」,
再加 `event_type='feature_used'` + `feature` 列,在对应命令里埋点(注意节流,别每个工具调用都发)。
