-- CaseBoard 匿名使用遥测 · Supabase 建表脚本
-- 目标:看「同事有没有在用 + 用了多久 + 用了之后会不会持续回来(留存)」
-- 隐私铁律:本表绝不存案件内容/当事人/文件名/API key,只有匿名设备ID + 计数/时长/版本。
-- 性质说明:按「设备」区分(不记名,但区分到每台安装),这是看留存所必需。
--
-- 用法:Supabase 控制台 → 左侧 SQL Editor → New query → 整段粘贴 → Run。
-- 跑一次即可。重复跑安全(IF NOT EXISTS / OR REPLACE)。

-- ============================================================
-- 1) 事件表
-- ============================================================
create table if not exists public.usage_events (
  id          bigint generated always as identity primary key,
  device_id   text not null,                 -- 复用 app 的匿名 client_id(UUID,跟人无关)
  app_version text,                           -- 如 "0.2.3"
  os          text,                           -- 粗粒度,如 "macos aarch64"(不带系统版本号)
  event_type  text not null,                  -- v1 只有两种:'session_start' | 'heartbeat'
  session_id  text,                           -- 一次开机的随机ID,用来去重/数会话数
  created_at  timestamptz not null default now()  -- 服务器时间(不信任客户端时钟)
);

create index if not exists usage_events_device_time_idx
  on public.usage_events (device_id, created_at);

-- ============================================================
-- 2) 行级安全(RLS):同事的 app 只能写,不能读别人数据
-- ============================================================
alter table public.usage_events enable row level security;

-- 允许匿名 key(同事 app 持有)插入
drop policy if exists "anon can insert events" on public.usage_events;
create policy "anon can insert events"
  on public.usage_events
  for insert
  to anon
  with check (true);

-- 不建任何 SELECT policy → 匿名 key 读不到任何行(扒出 key 也只能写)。
-- 你(作者)在 Supabase 后台是登录态,绕过 RLS,能看全部。

-- ============================================================
-- 3) 留存看板视图(你登录后台直接看,省得手写 SQL)
-- ============================================================

-- ⚠️ 安全:public schema 的视图默认会被 PostgREST 暴露给匿名 key,且视图默认以
--    创建者权限运行 → 匿名 key 能借视图读到聚合数据,绕过表上的「只写不读」RLS。
--    两道防护:① security_invoker=on(视图以查询者身份执行,anon 无表 SELECT 即读不到)
--             ② 显式 revoke 掉 anon/authenticated 的视图权限(双保险)。
--    你(作者)在后台是 postgres/service 角色,绕过这些,照样能看全部。

-- 3a) 每台设备一行:首次/最近出现、活跃了几天、开了几次、估算总时长、版本
--     看留存就看这张:active_days 多 + last_seen 在最近 = 这台在持续用。
create or replace view public.device_summary
  with (security_invoker = on) as
select
  device_id,
  min(created_at)                               as first_seen,
  max(created_at)                               as last_seen,
  count(distinct date_trunc('day', created_at)) as active_days,   -- 活跃天数 = 留存核心指标
  count(distinct session_id)                    as sessions,       -- 开了多少次(去重)
  -- 时长「下界」:每个心跳代表已用满 5 分钟。系统少算最多 5 分钟/会话;
  -- < 5 分钟的会话此值=0(但 sessions 仍计 1)。看趋势/留存够用,别当精确工时。
  count(*) filter (where event_type = 'heartbeat') * 5
                                                as est_minutes_lower_bound,
  max(app_version)                              as last_version
from public.usage_events
group by device_id
order by last_seen desc;

-- 3b) 每日活跃设备数:看整体趋势(还有几个人在用,涨还是跌)
create or replace view public.daily_active
  with (security_invoker = on) as
select
  date_trunc('day', created_at)::date as day,
  count(distinct device_id)           as active_devices,
  count(distinct session_id)          as sessions
from public.usage_events
group by 1
order by 1 desc;

-- 双保险:显式收回匿名/登录用户对两个视图的所有权限(防 PostgREST 暴露)。
revoke all on public.device_summary from anon, authenticated;
revoke all on public.daily_active  from anon, authenticated;

-- est_minutes_lower_bound 是「大概在用多久」的粗代理(时长下界,每心跳算满5分钟);
-- 不是秒表,看趋势/留存足够,别当精确工时。
