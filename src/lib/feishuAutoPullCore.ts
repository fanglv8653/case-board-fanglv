export interface FeishuAutoPullConnection {
  connected: boolean;
  reauthorization_required: boolean;
}

export interface FeishuAutoPullDependencies {
  isOnline: () => boolean;
  now: () => number;
  getConnectionStatus: () => Promise<FeishuAutoPullConnection>;
  pullPreview: () => Promise<unknown>;
}

export interface FeishuAutoPullResult {
  attempted: boolean;
  pulled: boolean;
  reason: "pulled" | "offline" | "throttled" | "disconnected" | "failed";
}

export function createFeishuAutoPullCoordinator(
  dependencies: FeishuAutoPullDependencies,
  minimumIntervalMs: number,
) {
  let lastAttemptAt = 0;
  let inFlight: Promise<FeishuAutoPullResult> | null = null;

  return function run(): Promise<FeishuAutoPullResult> {
    if (inFlight) return inFlight;
    if (!dependencies.isOnline()) {
      return Promise.resolve({ attempted: false, pulled: false, reason: "offline" });
    }
    const startedAt = dependencies.now();
    if (lastAttemptAt > 0 && startedAt - lastAttemptAt < minimumIntervalMs) {
      return Promise.resolve({ attempted: false, pulled: false, reason: "throttled" });
    }

    inFlight = (async () => {
      try {
        const connection = await dependencies.getConnectionStatus();
        if (!connection.connected || connection.reauthorization_required) {
          return { attempted: false, pulled: false, reason: "disconnected" as const };
        }
        lastAttemptAt = startedAt;
        await dependencies.pullPreview();
        return { attempted: true, pulled: true, reason: "pulled" as const };
      } catch {
        return { attempted: true, pulled: false, reason: "failed" as const };
      } finally {
        inFlight = null;
      }
    })();
    return inFlight;
  };
}
