/**
 * 2026-05-26 V0.1.13 P3b · @dnd-kit sortable 卡片包装。
 *
 * 用法:CaseSnapshotView 把每张 CardSection 包在 SortableCard 里,通过 render
 * prop 注入 dragHandle(由本组件的 useSortable 提供 attributes + listeners)。
 * dragHandle 由 CardSection 内部渲染 GripVertical 按钮 + 套 listeners,
 * 这样**只有按拖把手才能拖**,卡片其他位置(EyeOff / × / 字段)能正常点击编辑。
 *
 * 关键设计点:
 *   - PointerSensor 在 SortableContext 外侧装(CaseSnapshotView),距离 5px 才激活,
 *     防止点字段被误判为拖
 *   - 拖把手 listeners 只挂在 GripVertical 按钮,不挂整张卡片
 *   - useSortable id 用 section 标题字符串(跟 resolveOrder / hidden_sections 同一套)
 */
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

import type { DragHandleProps } from "./atoms";

export interface SortableCardProps {
  id: string;
  /** 编辑模式下才暴露 dragHandle;非编辑态 children 不接 dragHandle(纯静态卡片) */
  isEditMode: boolean;
  /** render prop 接 dragHandle(可能 undefined 当非编辑态) */
  children: (props: { dragHandle?: DragHandleProps }) => React.ReactNode;
}

export function SortableCard({ id, isEditMode, children }: SortableCardProps) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id, disabled: !isEditMode });

  const style: React.CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.4 : 1,
    // 拖拽时给卡片浮起来的视觉
    zIndex: isDragging ? 10 : undefined,
  };

  return (
    <div ref={setNodeRef} style={style}>
      {children({
        dragHandle: isEditMode ? { attributes, listeners } : undefined,
      })}
    </div>
  );
}
