import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { useId, type ReactNode } from "react";
import { cn } from "@/lib/cn";

export interface SegmentedOption<T extends string> {
  value: T;
  label: ReactNode;
}

interface Props<T extends string> {
  value: T;
  options: readonly SegmentedOption<T>[];
  onValueChange: (value: T) => void;
  className?: string;
  buttonClassName?: string;
}

interface ContentProps {
  value: string;
  children: ReactNode;
  className?: string;
}

/** 选中底座在各选项间共享布局，保留不同文案宽度下的平滑位移。 */
export function SegmentedControl<T extends string>({
  value,
  options,
  onValueChange,
  className,
  buttonClassName,
}: Props<T>) {
  const indicatorId = `segmented-${useId()}`;
  const reduceMotion = useReducedMotion();

  return (
    <div className={cn("flex items-center rounded-full border border-bd bg-sb", className)}>
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            aria-pressed={active}
            onClick={() => onValueChange(option.value)}
            className={cn(
              "relative cursor-pointer rounded-full font-semibold transition-colors",
              buttonClassName,
              active ? "text-tx" : "text-tx2",
            )}
          >
            {active && (
              <motion.span
                layoutId={indicatorId}
                initial={false}
                transition={
                  reduceMotion
                    ? { duration: 0 }
                    : { type: "spring", stiffness: 460, damping: 34, mass: 0.55 }
                }
                className="segmented-active-shadow pointer-events-none absolute inset-0 z-0 rounded-full bg-srf"
              />
            )}
            <span className="relative z-10">{option.label}</span>
          </button>
        );
      })}
    </div>
  );
}

/** 分段切换后的内容过渡；离场层脱离布局，新内容可立即占位。 */
export function SegmentedContent({ value, children, className }: ContentProps) {
  const reduceMotion = useReducedMotion();
  const transition = reduceMotion
    ? { duration: 0 }
    : { duration: 0.18, ease: "easeOut" as const };

  return (
    <div className={cn("relative", className)}>
      <AnimatePresence initial={false} mode="popLayout">
        <motion.div
          key={value}
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -3 }}
          transition={transition}
        >
          {children}
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
