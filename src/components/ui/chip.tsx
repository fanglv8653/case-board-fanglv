import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

/**
 * 统一的「胶囊标签 / 小按钮」组件(收敛全站手搓的 px/py/rounded-full chip 样式)。
 *
 * 用法:`<Chip>v0.3.0</Chip>` 或可点的 `<Chip asChild><button .../></Chip>`。
 * 收敛对象:VersionChip / DeepSeekBalanceChip / FeedbackButton 等处重复的胶囊样式。
 */
const chipVariants = cva(
  "inline-flex items-center gap-1 rounded-full border whitespace-nowrap transition-colors",
  {
    variants: {
      variant: {
        default:
          "border-border bg-card text-muted-foreground hover:bg-accent hover:text-foreground",
        muted: "border-border/60 bg-card/80 text-muted-foreground",
        warning:
          "border-amber-300/70 bg-amber-50 text-amber-800 dark:bg-amber-950/30 dark:text-amber-200",
        danger:
          "border-destructive/30 bg-destructive/5 text-destructive",
      },
      size: {
        sm: "px-2 py-0.5 text-caption",
        md: "px-2.5 py-0.5 text-label",
        lg: "px-3 py-1 text-xs",
      },
    },
    defaultVariants: { variant: "default", size: "md" },
  },
);

function Chip({
  className,
  variant,
  size,
  asChild = false,
  ...props
}: React.ComponentProps<"span"> &
  VariantProps<typeof chipVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot : "span";
  return (
    <Comp
      data-slot="chip"
      className={cn(chipVariants({ variant, size, className }))}
      {...props}
    />
  );
}

export { Chip };
