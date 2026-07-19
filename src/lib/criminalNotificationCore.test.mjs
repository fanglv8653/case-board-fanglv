import assert from "node:assert/strict";
import test from "node:test";

import { createReminderScanCoordinator } from "./criminalNotificationCore.ts";

function delivery(id = "delivery-1") {
  return {
    id,
    task_id: "task-1",
    case_id: "case-1",
    scheduled_for: "2026-07-18T10:00:00.000Z",
  };
}

function harness(overrides = {}) {
  const calls = { permission: 0, scan: 0, claim: 0, send: 0, marks: [] };
  const dependencies = {
    isEnabled: () => true,
    isPermissionGranted: async () => {
      calls.permission += 1;
      return true;
    },
    scanCandidates: async () => {
      calls.scan += 1;
      return 1;
    },
    claimDeliveries: async () => {
      calls.claim += 1;
      return [delivery()];
    },
    sendDelivery: async () => {
      calls.send += 1;
    },
    markDelivery: async (...args) => {
      calls.marks.push(args);
    },
    now: () => "2026-07-18T10:00:00.000Z",
    ...overrides,
  };
  return { calls, run: createReminderScanCoordinator(dependencies) };
}

test("关闭设置时不请求系统权限也不扫描", async () => {
  const { calls, run } = harness({ isEnabled: () => false });
  assert.deepEqual(await run(), {
    skipped: "disabled",
    claimed: 0,
    sent: 0,
    failed: 0,
  });
  assert.equal(calls.permission, 0);
  assert.equal(calls.scan, 0);
});

test("权限拒绝时不 claim，后台扫描不会循环申请权限", async () => {
  const { calls, run } = harness({ isPermissionGranted: async () => false });
  assert.equal((await run()).skipped, "permission");
  assert.equal(calls.scan, 0);
  assert.equal(calls.claim, 0);
});

test("并发重复触发复用同一轮扫描", async () => {
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  const { calls, run } = harness({
    scanCandidates: async () => {
      calls.scan += 1;
      await gate;
      return 1;
    },
  });
  const first = run();
  const second = run();
  release();
  await Promise.all([first, second]);
  assert.equal(calls.scan, 1);
  assert.equal(calls.claim, 1);
  assert.equal(calls.send, 1);
});

test("发送失败回写 failed，下一轮仍可重试", async () => {
  const { calls, run } = harness({
    sendDelivery: async () => {
      calls.send += 1;
      if (calls.send === 1) throw new Error("toast unavailable");
    },
  });
  assert.equal((await run()).failed, 1);
  assert.deepEqual(calls.marks[0], ["delivery-1", false, "toast unavailable"]);
  assert.equal((await run()).sent, 1);
  assert.deepEqual(calls.marks[1], ["delivery-1", true]);
});
