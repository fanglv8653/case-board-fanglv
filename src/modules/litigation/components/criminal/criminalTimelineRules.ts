type StageLike = {
  id: string;
  major_stage?: string | null;
  stage_label: string;
};

type DeadlineLike = {
  stage_item_id?: string | null;
  major_stage?: string | null;
};

type ApplicabilityDeadlineLike = {
  source_type?: string | null;
  applicability_status?: string | null;
};

type ApplicabilityInputLike = {
  applicability_status?: string | null;
  override_reason?: string | null;
};

function normalizeStageName(value: string | null | undefined) {
  return (value ?? "").trim().toLocaleLowerCase().replace(/\s+/g, "");
}

export function resolveDeadlineStageId(deadline: DeadlineLike, stages: StageLike[]) {
  if (deadline.stage_item_id) {
    const explicit = stages.find((stage) => stage.id === deadline.stage_item_id);
    if (explicit) return explicit.id;
  }

  const target = normalizeStageName(deadline.major_stage);
  if (!target) return null;

  const majorStageMatches = stages.filter(
    (stage) => normalizeStageName(stage.major_stage) === target,
  );
  if (majorStageMatches.length === 1) return majorStageMatches[0].id;
  if (majorStageMatches.length > 1) return null;

  const labelMatches = stages.filter(
    (stage) => normalizeStageName(stage.stage_label) === target,
  );
  return labelMatches.length === 1 ? labelMatches[0].id : null;
}

export function needsApplicabilityOverrideReason(
  original: ApplicabilityDeadlineLike | null | undefined,
  next: ApplicabilityInputLike,
) {
  if (!original || original.source_type !== "auto") return false;
  if (original.applicability_status === next.applicability_status) return false;
  return !next.override_reason?.trim();
}
