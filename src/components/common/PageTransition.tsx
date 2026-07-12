import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { type ReactNode } from "react";
import { cn } from "@/lib/cn";

interface Props {
  pageKey: string | null;
  children: ReactNode;
  className?: string;
}

/** 主内容页的轻量进出场过渡；离场页脱离布局以避免占据新页高度。 */
export function PageTransition({ pageKey, children, className }: Props) {
  const reduceMotion = useReducedMotion();
  const transition = reduceMotion
    ? { duration: 0 }
    : { duration: 0.2, ease: "easeOut" as const };

  return (
    <AnimatePresence initial={false} mode="popLayout">
      {pageKey && (
        <motion.div
          key={pageKey}
          initial={{ opacity: 0, x: 12 }}
          animate={{ opacity: 1, x: 0 }}
          exit={{ opacity: 0, x: -8 }}
          transition={transition}
          className={cn(
            "relative flex min-h-0 flex-1 flex-col will-change-[opacity,transform]",
            className,
          )}
        >
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  );
}
