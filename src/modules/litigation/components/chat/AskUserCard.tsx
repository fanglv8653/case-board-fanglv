/**
 * V0.3 · 选项式追问卡片(Claude Code 风格提问框)。
 *
 * 后端模型调 `ask_user` 工具时,前端把每个问题渲染成可点击的选项 + 可选自由输入框。
 * 用户选完/填完点「提交回答」,把「问 → 答」编号文本当作下一条普通 user 消息回灌,
 * 模型下一轮基于答案续写(大概率这轮就 save_artifact)。
 *
 * 单问题 + 有选项 + 不需自由输入 → 点一下选项即提交(一键,手感最顺)。
 * 多问题 / 需自由输入 → 逐题选/填 + 底部「提交回答」。
 */

import { useState } from "react";
import { CornerDownLeft } from "lucide-react";

import type { AskQuestion } from "@/lib/api";
import { cn } from "@/lib/utils";

interface Props {
  questions: AskQuestion[];
  disabled?: boolean;
  /** 用户提交后回灌的「问→答」文本(已编号);调用方据此发下一条 user 消息 */
  onSubmit: (answerText: string) => void;
}

/** 一题的当前作答:选中的预设项 + 自由输入;自由输入非空时优先生效 */
interface Answer {
  picked: string | null;
  text: string;
}

function effective(a: Answer): string {
  const t = a.text.trim();
  return t || a.picked || "";
}

/** 把所有「问→答」拼成回灌文本:单问省编号,多问编号 */
function composeAnswerText(questions: AskQuestion[], answers: Answer[]): string {
  const pairs = questions.map((q, i) => `${q.question} → ${effective(answers[i])}`);
  if (pairs.length === 1) return pairs[0];
  return pairs.map((p, i) => `${i + 1}. ${p}`).join("\n");
}

export function AskUserCard({ questions, disabled, onSubmit }: Props) {
  const [answers, setAnswers] = useState<Answer[]>(() =>
    questions.map(() => ({ picked: null, text: "" })),
  );

  // 单问题 + 有选项 + 不需自由输入 → 一键提交
  const oneClick =
    questions.length === 1 &&
    questions[0].options.length > 0 &&
    !questions[0].allow_input;

  const allAnswered = answers.every((a) => effective(a) !== "");

  function update(i: number, patch: Partial<Answer>) {
    setAnswers((cur) => cur.map((a, idx) => (idx === i ? { ...a, ...patch } : a)));
  }

  function pick(i: number, opt: string) {
    if (disabled) return;
    if (oneClick) {
      // 直接提交,不经 state(单题一键)
      onSubmit(`${questions[i].question} → ${opt}`);
      return;
    }
    // 再点同一项 = 取消选中(允许改主意)
    update(i, { picked: answers[i].picked === opt ? null : opt });
  }

  function submit() {
    if (disabled || !allAnswered) return;
    onSubmit(composeAnswerText(questions, answers));
  }

  return (
    <div className="mt-2 max-w-[95%] rounded-lg border border-sky-500/30 bg-sky-500/5 px-3 py-2.5 text-sm">
      <p className="mb-2 text-xs font-medium text-sky-700 dark:text-sky-300">
        请选择或填写(点选项即可{oneClick ? "" : ",填完点提交"}):
      </p>
      <div className="space-y-3">
        {questions.map((q, i) => {
          const showInput = q.allow_input || q.options.length === 0;
          return (
            <div key={i}>
              <p className="mb-1.5 text-foreground">
                {questions.length > 1 && (
                  <span className="mr-1 font-medium text-muted-foreground">
                    {i + 1}.
                  </span>
                )}
                {q.question}
              </p>
              {q.options.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {q.options.map((opt) => (
                    <button
                      key={opt}
                      type="button"
                      disabled={disabled}
                      onClick={() => pick(i, opt)}
                      className={cn(
                        "rounded-full border px-2.5 py-1 text-xs transition-colors disabled:opacity-40",
                        answers[i].picked === opt
                          ? "border-sky-500 bg-sky-500 text-white"
                          : "border-border bg-background hover:border-sky-500/50 hover:bg-sky-500/10",
                      )}
                    >
                      {opt}
                    </button>
                  ))}
                </div>
              )}
              {showInput && (
                <input
                  type="text"
                  value={answers[i].text}
                  disabled={disabled}
                  onChange={(e) => update(i, { text: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !oneClick && allAnswered) submit();
                  }}
                  placeholder={
                    q.options.length > 0 ? "或自己输入…" : "请输入…"
                  }
                  className="mt-1.5 w-full rounded-md border border-border bg-background px-2.5 py-1.5 text-xs outline-none transition-[border-color,box-shadow] focus:border-foreground focus:ring-1 focus:ring-foreground/20 disabled:opacity-40"
                />
              )}
            </div>
          );
        })}
      </div>
      {/* 始终给一个「直接写」出口(确定性,不依赖模型在选项里加)—— 老板:多问搜集背景,
          但用户随时可以让它别问了直接动手。点了就让模型基于现有信息起草、缺的留占位。 */}
      <div className="mt-3 flex items-center justify-between gap-2">
        <button
          type="button"
          disabled={disabled}
          onClick={() =>
            onSubmit(
              "信息够了,请直接根据现有信息起草,缺的关键项留 [占位] 待我补充,不用再问了。",
            )
          }
          className="text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline disabled:opacity-40"
        >
          信息够了,直接写 →
        </button>
        {!oneClick && (
          <button
            type="button"
            disabled={disabled || !allAnswered}
            onClick={submit}
            className="inline-flex items-center gap-1 rounded-md bg-sky-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-sky-700 disabled:cursor-not-allowed disabled:opacity-40"
          >
            <CornerDownLeft className="size-3.5" />
            提交回答
          </button>
        )}
      </div>
    </div>
  );
}
