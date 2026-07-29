export interface ConfirmedMemoryInjection {
  runId: string;
  previewSha256: string;
}

function key(caseId: string) {
  return `caseboard.memory-injection.once.${caseId}`;
}

export function saveConfirmedMemoryInjection(
  caseId: string,
  value: ConfirmedMemoryInjection,
) {
  sessionStorage.setItem(key(caseId), JSON.stringify(value));
  window.dispatchEvent(
    new CustomEvent("caseboard:memory-injection-confirmed", {
      detail: { caseId },
    }),
  );
}

export function readConfirmedMemoryInjection(
  caseId: string,
): ConfirmedMemoryInjection | null {
  try {
    const raw = sessionStorage.getItem(key(caseId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<ConfirmedMemoryInjection>;
    if (
      typeof parsed.runId !== "string" ||
      !parsed.runId ||
      typeof parsed.previewSha256 !== "string" ||
      !parsed.previewSha256
    ) {
      sessionStorage.removeItem(key(caseId));
      return null;
    }
    return { runId: parsed.runId, previewSha256: parsed.previewSha256 };
  } catch {
    sessionStorage.removeItem(key(caseId));
    return null;
  }
}

export function consumeConfirmedMemoryInjection(caseId: string) {
  sessionStorage.removeItem(key(caseId));
}
