import { useEffect, useState } from "react";
import { BookOpen, ChevronDown, Database, Search } from "lucide-react";

import { getLocalKbGuide } from "@/lib/api";
import type { LocalKbGuide } from "@/lib/types";

export function LocalKbGuideCard({ configuredRoot }: { configuredRoot: string | null }) {
  const [guide, setGuide] = useState<LocalKbGuide | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    void getLocalKbGuide()
      .then((value) => {
        if (active) setGuide(value);
      })
      .catch((reason) => {
        if (active) setError(String(reason));
      });
    return () => {
      active = false;
    };
  }, [configuredRoot]);

  const maxMb = guide ? guide.keyword_search.max_file_bytes / 1024 / 1024 : null;

  return (
    <section className="lg:col-span-2" aria-labelledby="local-kb-guide-title">
      <div className="mb-3">
        <h3 id="local-kb-guide-title" className="text-sm font-semibold text-foreground">
          检索与维护说明
        </h3>
        <p className="mt-0.5 text-xs text-muted-foreground">
          说明当前目录怎样被检索，以及本功能不会执行哪些写入操作。
        </p>
      </div>

      <div className="rounded-lg border border-border bg-background/50 p-4">
        <div className="grid gap-3 md:grid-cols-2">
          <div className="rounded-md border border-border bg-card/70 p-3">
            <div className="flex items-center gap-2">
              <Database className="size-4 text-emerald-700" aria-hidden="true" />
              <p className="text-xs font-semibold text-foreground">当前知识库</p>
            </div>
            <p className="mt-2 break-all font-mono text-label text-muted-foreground">
              {(guide?.configured_root ?? configuredRoot?.trim()) || "尚未绑定本地文件夹"}
            </p>
            {guide && !guide.root_available && guide.configured_root && (
              <p className="mt-1 text-xs text-amber-700">当前目录暂不可访问，请检查挂载或权限。</p>
            )}
          </div>
          <div className="rounded-md border border-border bg-card/70 p-3">
            <div className="flex items-center gap-2">
              <Search className="size-4 text-emerald-700" aria-hidden="true" />
              <p className="text-xs font-semibold text-foreground">真实检索范围</p>
            </div>
            <p className="mt-2 text-label leading-5 text-muted-foreground">
              {guide
                ? `${guide.keyword_search.scope}；文件类型 ${guide.keyword_search.extensions
                    .map((item) => `.${item}`)
                    .join("、")}，单文件不超过 ${maxMb} MB。`
                : "正在读取后端实际检索规则…"}
            </p>
          </div>
        </div>
        {error && <p className="mt-3 text-xs text-destructive">读取检索说明失败：{error}</p>}

        <details className="group mt-3 rounded-md border border-border bg-card/50">
          <summary className="flex cursor-pointer list-none items-center justify-between gap-3 px-3 py-2.5 text-xs font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
            <span className="flex items-center gap-2">
              <BookOpen className="size-4 text-muted-foreground" aria-hidden="true" />
              查看完整只读说明
            </span>
            <ChevronDown
              className="size-4 text-muted-foreground transition-transform group-open:rotate-180"
              aria-hidden="true"
            />
          </summary>
          <div className="space-y-4 border-t border-border px-4 py-3 text-xs leading-5 text-muted-foreground">
            <section>
              <h4 className="font-semibold text-foreground">两种检索方式</h4>
              <ol className="mt-1 list-decimal space-y-1 pl-5">
                <li>
                  <strong className="text-foreground">关键词检索：</strong>
                  {guide
                    ? `${guide.keyword_search.scope}；${guide.keyword_search.sorting.join("、")}。`
                    : "读取后端规则中…"}
                </li>
                <li>
                  <strong className="text-foreground">语义检索：</strong>
                  {guide
                    ? `${guide.semantic_search.scope}；${guide.semantic_search.mismatch_behavior}。`
                    : "读取后端规则中…"}
                </li>
              </ol>
            </section>

            <section>
              <h4 className="font-semibold text-foreground">默认排除</h4>
              <ul className="mt-1 list-disc space-y-1 pl-5">
                {guide?.keyword_search.excluded_root_prefixes.map((item) => (
                  <li key={`root-${item}`}>{item}</li>
                ))}
                {guide?.keyword_search.excluded_segments.map((item) => (
                  <li key={`segment-${item}`}>{item}</li>
                ))}
                {!guide && <li>正在读取后端排除规则…</li>}
              </ul>
            </section>

            <section>
              <h4 className="font-semibold text-foreground">读取与维护边界</h4>
              <ul className="mt-1 list-disc space-y-1 pl-5">
                {guide?.maintenance_boundaries.map((item) => <li key={item}>{item}</li>)}
                {!guide && <li>正在读取后端维护边界…</li>}
              </ul>
            </section>
          </div>
        </details>
      </div>
    </section>
  );
}
