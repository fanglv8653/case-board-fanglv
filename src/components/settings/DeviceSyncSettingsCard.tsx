import { useCallback, useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ArchiveRestore,
  CheckCircle2,
  FolderOpen,
  Link2,
  Loader2,
  Pause,
  Play,
  RefreshCw,
  ShieldCheck,
  UserMinus,
  Users,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { confirmDialog } from "@/lib/dialog";
import {
  approveDeviceSyncJoin,
  completeDeviceSyncJoin,
  createDeviceSyncGroup,
  createDeviceSyncInvite,
  createDeviceSyncJoinRequest,
  createDeviceSyncSnapshot,
  getDeviceSyncStatus,
  listDeviceSyncConflicts,
  listDeviceSyncMembers,
  listDeviceSyncManualReviews,
  listDeviceSyncSnapshots,
  previewDeviceSyncRecovery,
  previewDeviceSyncRestore,
  resolveDeviceSyncConflict,
  reviewDeviceSyncManualQuarantine,
  revokeDeviceSyncMember,
  runDeviceSync,
  setDeviceSyncPaused,
  validateDeviceSyncNasPath,
} from "@/lib/api";
import type {
  DeviceSyncConflict,
  DeviceSyncInvite,
  DeviceSyncJoinRequest,
  DeviceSyncMember,
  DeviceSyncManualReview,
  DeviceSyncRecoveryPreview,
  DeviceSyncRestorePreview,
  DeviceSyncSnapshot,
  DeviceSyncStatus,
} from "@/lib/types";

const inputClass =
  "w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20";
const subCard = "rounded-lg border border-border bg-background p-3";

const INCLUDED = [
  "案件基本信息、案件实例与程序阶段",
  "期限、任务、待办和工作记录",
  "收入记录、收付款记录及核算字段",
  "飞书关联、同步快照、冲突与收件箱",
  "全局法律 Skills 包及启用关系",
];
const EXCLUDED = [
  "原始材料、附件路径、OCR/抽取全文",
  "聊天记录与案件记忆",
  "任何 API Key、令牌、密码和原始飞书载荷",
  "SQLite/WAL/SHM 数据库文件",
];

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function syncTime(value: string | null) {
  if (!value) return "无";
  const parsed = new Date(value.endsWith("Z") ? value : `${value}Z`);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

async function chooseDirectory(): Promise<string | null> {
  const result = await open({ directory: true, multiple: false });
  return typeof result === "string" ? result : null;
}

async function chooseFile(): Promise<string | null> {
  const result = await open({ directory: false, multiple: false });
  return typeof result === "string" ? result : null;
}

export function DeviceSyncSettingsCard() {
  const [status, setStatus] = useState<DeviceSyncStatus | null>(null);
  const [members, setMembers] = useState<DeviceSyncMember[]>([]);
  const [manualReviews, setManualReviews] = useState<DeviceSyncManualReview[]>([]);
  const [conflicts, setConflicts] = useState<DeviceSyncConflict[]>([]);
  const [snapshots, setSnapshots] = useState<DeviceSyncSnapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const [nasPath, setNasPath] = useState("");
  const [deviceName, setDeviceName] = useState("");
  const [recoveryPath, setRecoveryPath] = useState("");
  const [recoveryPassphrase, setRecoveryPassphrase] = useState("");
  const [invite, setInvite] = useState<DeviceSyncInvite | null>(null);
  const [pairingCode, setPairingCode] = useState("");
  const [joinRequest, setJoinRequest] = useState<DeviceSyncJoinRequest | null>(null);
  const [requestPath, setRequestPath] = useState("");
  const [expectedFingerprint, setExpectedFingerprint] = useState("");
  const [completionPath, setCompletionPath] = useState("");
  const [snapshotPath, setSnapshotPath] = useState("");
  const [restorePreview, setRestorePreview] = useState<DeviceSyncRestorePreview | null>(null);
  const [recoveryPackagePath, setRecoveryPackagePath] = useState("");
  const [recoveryPreview, setRecoveryPreview] = useState<DeviceSyncRecoveryPreview | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const next = await getDeviceSyncStatus();
      setStatus(next);
      if (next) {
        const [nextMembers, nextConflicts, nextSnapshots, nextManualReviews] = await Promise.all([
          listDeviceSyncMembers(next.group_id),
          listDeviceSyncConflicts(next.group_id),
          listDeviceSyncSnapshots(next.group_id),
          listDeviceSyncManualReviews(next.group_id),
        ]);
        setMembers(nextMembers);
        setConflicts(nextConflicts);
        setSnapshots(nextSnapshots);
        setManualReviews(nextManualReviews);
        setNasPath(next.connector_root);
      } else {
        setMembers([]);
        setConflicts([]);
        setSnapshots([]);
        setManualReviews([]);
      }
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function run(key: string, action: () => Promise<unknown>, message: string) {
    setBusy(key);
    setError("");
    setNotice("");
    try {
      await action();
      setNotice(message);
      await refresh();
    } catch (e) {
      const message = errorText(e);
      await refresh();
      setError(message);
    } finally {
      setBusy("");
    }
  }

  async function selectRecoveryDestination() {
    const result = await save({
      defaultPath: "caseboard-device-sync-recovery.json",
      filters: [{ name: "加密恢复包", extensions: ["json"] }],
    });
    if (typeof result === "string") setRecoveryPath(result);
  }

  if (loading) {
    return <div className="flex items-center gap-2 text-sm text-muted-foreground"><Loader2 className="size-4 animate-spin" />读取设备同步状态…</div>;
  }

  return (
    <div className="space-y-4">
      <div className="rounded-lg border border-amber-500/40 bg-amber-500/5 p-3">
        <div className="flex items-start gap-2">
          <ShieldCheck className="mt-0.5 size-5 shrink-0 text-amber-600" />
          <div>
            <div className="font-medium">NAS 只是加密中转目录，不是数据库共享盘</div>
            <p className="mt-1 text-xs text-muted-foreground">
              两台电脑各自保留正式本地数据库，通过 NAS 中的签名加密变更包双向同步。未知字段、签名异常和冲突均失败关闭。
            </p>
          </div>
        </div>
      </div>

      {error && <div className="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm text-destructive">{error}</div>}
      {notice && <div className="rounded-md border border-emerald-500/30 bg-emerald-500/5 p-3 text-sm">{notice}</div>}

      <div className={subCard}>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="font-medium">连接与运行状态</div>
            {status ? (
              <p className="mt-1 text-xs text-muted-foreground">
                {status.auto_paused ? "已自动暂停" : status.paused ? "已手动暂停" : "运行中"} · 待上传 {status.pending_upload} · 冲突 {status.conflicts} · 活动隔离 {status.quarantined} · 待人工复核 {status.manual_review} · 密钥代次 {status.key_epoch}
              </p>
            ) : (
              <p className="mt-1 text-xs text-muted-foreground">尚未创建或加入设备同步组。NAS 未挂载时可暂不配置。</p>
            )}
          </div>
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={() => void refresh()}><RefreshCw />刷新</Button>
            {status && (
              <>
                <Button variant="outline" size="sm" disabled={Boolean(busy)} onClick={() => void run("pause", () => setDeviceSyncPaused(status.group_id, !status.paused), status.paused ? "同步已恢复。" : "同步已暂停。")}>
                  {status.paused ? <Play /> : <Pause />}{status.paused ? "恢复" : "暂停"}
                </Button>
                <Button size="sm" disabled={Boolean(busy) || status.paused} onClick={() => void run("sync", () => runDeviceSync(status.group_id), "手动双向同步完成。")}>
                  立即同步
                </Button>
              </>
            )}
          </div>
        </div>
        {status && (
          <p className="mt-2 text-xs text-muted-foreground">
            最近尝试：{syncTime(status.last_attempt_at)} · 最近成功：{syncTime(status.last_success_at)}
            {status.pause_reason_code ? ` · 暂停原因：${status.pause_reason_code}` : ""}
          </p>
        )}
        <div className="mt-3 flex gap-2">
          <input className={inputClass} value={nasPath} onChange={(e) => setNasPath(e.target.value)} placeholder="NAS 挂载盘符路径或 UNC 路径" />
          <Button variant="outline" onClick={async () => { const path = await chooseDirectory(); if (path) setNasPath(path); }}><FolderOpen />选择</Button>
          <Button variant="outline" disabled={!nasPath.trim() || Boolean(busy)} onClick={() => void run("validate", () => validateDeviceSyncNasPath(nasPath.trim()), "NAS 目录可访问、可写入。")}>验证绑定</Button>
        </div>
      </div>

      {status && manualReviews.length > 0 && (
        <div className={subCard}>
          <div className="flex items-center gap-2 font-medium"><AlertTriangle className="size-4 text-amber-600" />旧版隔离记录待人工复核</div>
          <p className="mt-1 text-xs text-muted-foreground">仅显示脱敏编号、稳定原因码与时间；旧路径和底层错误正文不会返回前端。</p>
          <div className="mt-3 space-y-2">
            {manualReviews.map((item) => (
              <div key={item.id} className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border p-2 text-xs">
                <span>{item.reason_code} · 首次 {syncTime(item.first_seen_at)} · 重试 {item.retry_count}</span>
                <div className="flex gap-2">
                  <Button variant="outline" size="sm" disabled={Boolean(busy)} onClick={() => void run(`retain-${item.id}`, () => reviewDeviceSyncManualQuarantine(status.group_id, item.id, "retain"), "已保留待复核并记录审计。")}>保留待核</Button>
                  <Button variant="outline" size="sm" disabled={Boolean(busy)} onClick={() => void run(`archive-${item.id}`, () => reviewDeviceSyncManualQuarantine(status.group_id, item.id, "archive"), "已确认归档并记录审计。")}>确认归档</Button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {!status && (
        <div className={subCard}>
          <div className="font-medium">创建首个同步组</div>
          <p className="mt-1 text-xs text-muted-foreground">创建时必须同时导出离线加密恢复包；恢复包须保存在 NAS 同步目录之外，且不会覆盖已有文件。</p>
          <div className="mt-3 grid gap-2 md:grid-cols-2">
            <input className={inputClass} value={deviceName} onChange={(e) => setDeviceName(e.target.value)} placeholder="本机设备名称" />
            <input className={inputClass} type="password" value={recoveryPassphrase} onChange={(e) => setRecoveryPassphrase(e.target.value)} placeholder="恢复包口令（至少 12 个字符）" />
            <input className={inputClass} value={recoveryPath} readOnly placeholder="离线恢复包保存位置" />
            <Button variant="outline" onClick={() => void selectRecoveryDestination()}><ArchiveRestore />选择恢复包位置</Button>
          </div>
          <Button className="mt-3" disabled={Boolean(busy) || !nasPath.trim() || !deviceName.trim() || !recoveryPath || recoveryPassphrase.length < 12} onClick={() => void run(
            "create",
            () => createDeviceSyncGroup({
              connector_root: nasPath.trim(),
              display_name: deviceName.trim(),
              recovery_destination: recoveryPath,
              recovery_passphrase: recoveryPassphrase,
            }),
            "同步组已创建，离线恢复包已导出。",
          )}>创建同步组</Button>
        </div>
      )}

      <div className={subCard}>
        <div className="flex items-center gap-2 font-medium"><Link2 className="size-4" />邀请、加入与审批</div>
        {status && (
          <div className="mt-3">
            <Button variant="outline" disabled={Boolean(busy) || status.paused} onClick={async () => {
              setBusy("invite");
              setError("");
              try { setInvite(await createDeviceSyncInvite(status.group_id)); } catch (e) { setError(errorText(e)); } finally { setBusy(""); }
            }}>创建一次性邀请</Button>
            {invite && <div className="mt-2 rounded-md bg-muted p-3 text-sm">配对码：<strong className="font-mono">{invite.pairing_code}</strong><br /><span className="text-xs text-muted-foreground">有效期至 {invite.expires_at}，请通过可信渠道交给另一台电脑。</span></div>}
          </div>
        )}
        <div className="mt-3 grid gap-2 md:grid-cols-3">
          <input className={inputClass} value={pairingCode} onChange={(e) => setPairingCode(e.target.value)} placeholder="另一台电脑收到的配对码" />
          <input className={inputClass} value={deviceName} onChange={(e) => setDeviceName(e.target.value)} placeholder="加入设备名称" />
          <Button variant="outline" disabled={!nasPath || !pairingCode || !deviceName || Boolean(busy)} onClick={async () => {
            setBusy("join-request");
            setError("");
            try { setJoinRequest(await createDeviceSyncJoinRequest({ connector_root: nasPath, pairing_code: pairingCode, display_name: deviceName })); } catch (e) { setError(errorText(e)); } finally { setBusy(""); }
          }}>生成加入申请</Button>
        </div>
        {joinRequest && <div className="mt-2 text-xs text-muted-foreground">已生成申请 {joinRequest.request_id}；设备指纹 <span className="font-mono">{joinRequest.fingerprint}</span>，请通过可信渠道交给审批设备。</div>}
        <div className="mt-3 grid gap-2 md:grid-cols-3">
          <input className={inputClass} value={requestPath} onChange={(e) => setRequestPath(e.target.value)} placeholder="加入申请文件路径" />
          <Button variant="outline" onClick={async () => { const path = await chooseFile(); if (path) setRequestPath(path); }}><FolderOpen />选择申请</Button>
          <input className={inputClass} value={expectedFingerprint} onChange={(e) => setExpectedFingerprint(e.target.value)} placeholder="通过可信渠道核对并输入设备指纹" />
          <Button variant="outline" disabled={!status || !requestPath || !expectedFingerprint.trim() || Boolean(busy)} onClick={async () => {
            if (!status) return;
            setBusy("approve");
            setError("");
            setNotice("");
            try {
              const result = await approveDeviceSyncJoin(status.group_id, requestPath, expectedFingerprint.trim());
              setCompletionPath(result.completion_path);
              setNotice("加入申请已审批，完成包路径已自动填入。");
              await refresh();
            } catch (e) {
              setError(errorText(e));
            } finally {
              setBusy("");
            }
          }}>审批加入</Button>
          <input className={inputClass} value={completionPath} onChange={(e) => setCompletionPath(e.target.value)} placeholder="审批完成包路径" />
          <Button variant="outline" onClick={async () => { const path = await chooseFile(); if (path) setCompletionPath(path); }}><FolderOpen />选择完成包</Button>
          <Button variant="outline" disabled={!nasPath || !requestPath || !completionPath || !pairingCode || Boolean(busy)} onClick={() => void run("complete", () => completeDeviceSyncJoin({ connector_root: nasPath, request_path: requestPath, completion_path: completionPath, pairing_code: pairingCode }), "本机已完成加入，请刷新状态。")}>完成加入</Button>
        </div>
      </div>

      {status && (
        <>
          <div className={subCard}>
            <div className="flex items-center gap-2 font-medium"><Users className="size-4" />受信设备</div>
            <div className="mt-3 space-y-2">
              {members.map((member) => (
                <div key={member.device_id} className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border p-2 text-sm">
                  <div>{member.display_name} <span className="text-xs text-muted-foreground">{member.fingerprint} · {member.status}</span></div>
                  {member.device_id !== status.local_device_id && member.status === "trusted" && (
                    <Button variant="outline" size="sm" disabled={Boolean(busy)} onClick={async () => {
                      const ok = await confirmDialog("吊销后该设备将不能继续读写同步组，并会触发密钥轮换。是否继续？", { danger: true, okLabel: "吊销设备" });
                      if (ok) void run("revoke", () => revokeDeviceSyncMember(status.group_id, member.device_id), "设备已吊销并完成密钥轮换。");
                    }}><UserMinus />吊销</Button>
                  )}
                </div>
              ))}
            </div>
          </div>

          <div className={subCard}>
            <div className="flex items-center gap-2 font-medium"><AlertTriangle className="size-4" />待处理冲突</div>
            <p className="mt-1 text-xs text-muted-foreground">“NAS 版本”指另一台受信设备写入的加密远端版本；系统不会静默覆盖本机值。</p>
            <div className="mt-3 space-y-2">
              {conflicts.filter((item) => item.status === "pending").map((item) => (
                <div key={item.id} className="rounded-md border border-border p-3 text-sm">
                  <div className="font-medium">{item.entity_type}/{item.entity_id} · {item.field_key}</div>
                  <div className="mt-2 grid gap-2 md:grid-cols-2">
                    <pre className="overflow-auto whitespace-pre-wrap rounded bg-muted p-2 text-xs">本机：{item.local_value_json ?? "空"}</pre>
                    <pre className="overflow-auto whitespace-pre-wrap rounded bg-muted p-2 text-xs">NAS：{item.remote_value_json ?? "空"}</pre>
                  </div>
                  <div className="mt-2 flex gap-2">
                    <Button size="sm" variant="outline" disabled={Boolean(busy)} onClick={() => void run("local", () => resolveDeviceSyncConflict(item.operation_id, "keep_local"), "冲突已选择保留本机版本。")}>保留本机版本</Button>
                    <Button size="sm" variant="outline" disabled={Boolean(busy)} onClick={() => void run("remote", () => resolveDeviceSyncConflict(item.operation_id, "keep_remote"), "冲突已选择 NAS 版本。")}>采用 NAS 版本</Button>
                  </div>
                </div>
              ))}
              {!conflicts.some((item) => item.status === "pending") && <p className="text-sm text-muted-foreground">暂无待处理冲突。</p>}
            </div>
          </div>

          <div className={subCard}>
            <div className="flex items-center gap-2 font-medium"><ArchiveRestore className="size-4" />加密快照与隔离恢复预览</div>
            <p className="mt-1 text-xs text-muted-foreground">预览只在隔离区解密和比对，不写入正式数据库；本页首期不提供“一键覆盖恢复”。</p>
            <Button className="mt-3" variant="outline" disabled={Boolean(busy)} onClick={() => void run("snapshot", () => createDeviceSyncSnapshot(status.group_id), "手动加密快照已创建。")}>创建手动快照</Button>
            <div className="mt-3 space-y-1 text-xs text-muted-foreground">
              {snapshots.map((item) => <div key={item.snapshot_id}>{item.snapshot_id} · {item.encrypted_path}</div>)}
            </div>
            <div className="mt-3 flex gap-2">
              <input className={inputClass} value={snapshotPath} onChange={(e) => { setSnapshotPath(e.target.value); setRestorePreview(null); }} placeholder="选择加密快照文件" />
              <Button variant="outline" onClick={async () => { const path = await chooseFile(); if (path) { setSnapshotPath(path); setRestorePreview(null); } }}><FolderOpen />选择</Button>
              <Button variant="outline" disabled={!snapshotPath || Boolean(busy)} onClick={async () => {
                setBusy("restore-preview"); setError("");
                try { setRestorePreview(await previewDeviceSyncRestore(status.group_id, snapshotPath)); } catch (e) { setError(errorText(e)); } finally { setBusy(""); }
              }}>隔离预览</Button>
            </div>
            {restorePreview && <div className="mt-2 rounded-md border border-emerald-500/30 bg-emerald-500/5 p-2 text-xs"><CheckCircle2 className="mr-1 inline size-4" />正式数据库未改变：{String(restorePreview.formal_database_unchanged)}；新增实体 {Object.values(restorePreview.new_entities).reduce((a, b) => a + b, 0)} 个。</div>}
          </div>
        </>
      )}

      <div className={subCard}>
        <div className="font-medium">离线恢复包预览</div>
        <p className="mt-1 text-xs text-muted-foreground">仅验证包格式、签名和口令并显示成员/密钥代次，不在本页直接恢复正式数据。</p>
        <div className="mt-3 grid gap-2 md:grid-cols-3">
          <input className={inputClass} value={recoveryPackagePath} onChange={(e) => setRecoveryPackagePath(e.target.value)} placeholder="恢复包路径" />
          <Button variant="outline" onClick={async () => { const path = await chooseFile(); if (path) setRecoveryPackagePath(path); }}><FolderOpen />选择</Button>
          <Button variant="outline" disabled={!recoveryPackagePath || recoveryPassphrase.length < 12 || Boolean(busy)} onClick={async () => {
            setBusy("recovery-preview"); setError("");
            try { setRecoveryPreview(await previewDeviceSyncRecovery(recoveryPackagePath, recoveryPassphrase)); } catch (e) { setError(errorText(e)); } finally { setBusy(""); }
          }}>验证并预览</Button>
        </div>
        {recoveryPreview && <div className="mt-2 text-xs text-muted-foreground">同步组 {recoveryPreview.group_id} · 最新密钥代次 {recoveryPreview.latest_key_epoch} · 受信设备 {recoveryPreview.trusted_members.length} · 正式数据库未改变</div>}
      </div>

      <div className={subCard}>
        <div className="font-medium">固定同步白名单与排除项</div>
        <div className="mt-3 grid gap-4 md:grid-cols-2">
          <div><div className="text-sm font-medium text-emerald-700">纳入同步</div><ul className="mt-2 space-y-1 text-xs text-muted-foreground">{INCLUDED.map((item) => <li key={item}>✓ {item}</li>)}</ul></div>
          <div><div className="text-sm font-medium text-rose-700">首期明确排除</div><ul className="mt-2 space-y-1 text-xs text-muted-foreground">{EXCLUDED.map((item) => <li key={item}>× {item}</li>)}</ul></div>
        </div>
      </div>
    </div>
  );
}
