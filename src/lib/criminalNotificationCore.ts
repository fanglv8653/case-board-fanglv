export interface ReminderDeliveryLike {
  id: string;
  task_id: string;
  case_id: string;
  scheduled_for: string;
}

export interface ReminderScanDependencies<T extends ReminderDeliveryLike> {
  isEnabled: () => boolean;
  isPermissionGranted: () => Promise<boolean>;
  scanCandidates: (now: string) => Promise<number>;
  claimDeliveries: (now: string) => Promise<T[]>;
  sendDelivery: (delivery: T) => void | Promise<void>;
  markDelivery: (
    deliveryId: string,
    sent: boolean,
    errorMessage?: string,
  ) => Promise<unknown>;
  now: () => string;
}

export interface ReminderScanResult {
  skipped: "disabled" | "permission" | null;
  claimed: number;
  sent: number;
  failed: number;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * 单飞扫描器：启动、聚焦和定时器即使同时触发，也只会执行同一轮 claim/send。
 * 发送失败会回写 failed，下一轮可由后端重新认领；发送成功仅表示系统通知已交付，
 * 不代表用户已经阅读。
 */
export function createReminderScanCoordinator<T extends ReminderDeliveryLike>(
  dependencies: ReminderScanDependencies<T>,
) {
  let activeScan: Promise<ReminderScanResult> | null = null;

  async function execute(): Promise<ReminderScanResult> {
    if (!dependencies.isEnabled()) {
      return { skipped: "disabled", claimed: 0, sent: 0, failed: 0 };
    }
    if (!(await dependencies.isPermissionGranted())) {
      return { skipped: "permission", claimed: 0, sent: 0, failed: 0 };
    }

    const now = dependencies.now();
    await dependencies.scanCandidates(now);
    const deliveries = await dependencies.claimDeliveries(now);
    let sent = 0;
    let failed = 0;

    for (const delivery of deliveries) {
      try {
        await dependencies.sendDelivery(delivery);
        await dependencies.markDelivery(delivery.id, true);
        sent += 1;
      } catch (error) {
        failed += 1;
        try {
          await dependencies.markDelivery(delivery.id, false, errorText(error));
        } catch {
          // 后端写回失败由下一次运行和诊断日志暴露；不能因此阻断同批其他提醒。
        }
      }
    }

    return {
      skipped: null,
      claimed: deliveries.length,
      sent,
      failed,
    };
  }

  return function runScan(): Promise<ReminderScanResult> {
    if (activeScan) return activeScan;
    activeScan = execute().finally(() => {
      activeScan = null;
    });
    return activeScan;
  };
}

