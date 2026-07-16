import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import type { CSSProperties } from "react";
import { Icon, type IconName } from "./Icon";

interface AnimatedIconProps {
  name: IconName;
  size?: number;
  strokeWidth?: number;
  className?: string;
  style?: CSSProperties;
  variant?: "crossfade" | "pop";
}

/** 在不改变按钮尺寸的前提下，为状态图标提供统一的切换反馈。 */
export function AnimatedIcon({
  name,
  size = 20,
  strokeWidth,
  className,
  style,
  variant = "crossfade",
}: AnimatedIconProps) {
  const reduceMotion = useReducedMotion();
  const pop = variant === "pop";

  return (
    <span
      aria-hidden="true"
      className={`relative inline-grid shrink-0 place-items-center ${className ?? ""}`}
      style={{ width: size, height: size }}
    >
      <AnimatePresence initial={false}>
        <motion.span
          key={name}
          className="absolute inset-0 grid place-items-center"
          initial={reduceMotion ? { opacity: 0 } : { opacity: 0, scale: pop ? 0.58 : 0.82 }}
          animate={{ opacity: 1, scale: 1 }}
          exit={reduceMotion ? { opacity: 0 } : { opacity: 0, scale: pop ? 1.28 : 1.12 }}
          transition={
            reduceMotion
              ? { duration: 0.01 }
              : pop
                ? { type: "spring", stiffness: 520, damping: 24, mass: 0.6 }
                : { duration: 0.16, ease: [0.22, 1, 0.36, 1] }
          }
        >
          <Icon name={name} size={size} strokeWidth={strokeWidth} style={style} />
        </motion.span>
      </AnimatePresence>
    </span>
  );
}
