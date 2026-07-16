import { AnimatePresence, motion, useAnimationControls, useReducedMotion } from "framer-motion";
import { useEffect, useRef } from "react";
import { cn } from "@/lib/cn";
import type { RepeatMode } from "@/types/player";

const CONTROL_BUTTON =
  "grid shrink-0 cursor-pointer place-items-center rounded-full transition-[color,background-color] duration-200 hover:bg-hv focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ac";
const MOTION_EASE = [0.22, 1, 0.36, 1] as const;

interface ControlButtonProps {
  label: string;
  onClick: () => void;
  buttonSize: number;
  iconSize: number;
  className?: string;
}

interface ShuffleControlButtonProps extends ControlButtonProps {
  active: boolean;
}

export function ShuffleControlIcon({ active, size }: { active: boolean; size: number }) {
  const reduceMotion = useReducedMotion();
  const previousActive = useRef(active);
  const shellControls = useAnimationControls();
  const upperControls = useAnimationControls();
  const lowerControls = useAnimationControls();

  useEffect(() => {
    if (previousActive.current === active) return;
    previousActive.current = active;
    if (reduceMotion) return;

    const direction = active ? 1 : -1;
    void shellControls.start({
      scale: [1, 0.84, 1.05, 1],
      transition: { duration: 0.3, times: [0, 0.34, 0.72, 1], ease: MOTION_EASE },
    });
    void upperControls.start({
      x: [0, 2 * direction, -0.5 * direction, 0],
      transition: { duration: 0.3, times: [0, 0.34, 0.72, 1], ease: MOTION_EASE },
    });
    void lowerControls.start({
      x: [0, -2 * direction, 0.5 * direction, 0],
      transition: { duration: 0.3, times: [0, 0.34, 0.72, 1], ease: MOTION_EASE },
    });
  }, [active, lowerControls, reduceMotion, shellControls, upperControls]);

  return (
    <motion.span
      aria-hidden="true"
      animate={shellControls}
      whileHover={reduceMotion ? undefined : { scale: 1.08 }}
      className="inline-grid shrink-0 place-items-center"
      style={{ width: size, height: size }}
    >
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <motion.g animate={upperControls}>
          <path d="M3.5 6.3H6c2.3 0 3.8.9 5.1 2.7l5.2 7.1c.8 1.1 1.7 1.5 3.2 1.5H21" />
          <path d="m17.8 14.5 3.2 3.1-3.2 3.1" />
        </motion.g>
        <motion.g animate={lowerControls}>
          <path d="M3.5 17.7H6c2.3 0 3.8-.9 5.1-2.7l5.2-7.1c.8-1.1 1.7-1.5 3.2-1.5H21" />
          <path d="m17.8 3.3 3.2 3.1-3.2 3.1" />
        </motion.g>
      </svg>
    </motion.span>
  );
}

export function ShuffleControlButton({
  active,
  label,
  onClick,
  buttonSize,
  iconSize,
  className,
}: ShuffleControlButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={active}
      data-active={active}
      onClick={onClick}
      className={cn(
        CONTROL_BUTTON,
        active ? "text-ac" : "text-tx2 hover:text-tx",
        className,
      )}
      style={{ width: buttonSize, height: buttonSize }}
    >
      <ShuffleControlIcon active={active} size={iconSize} />
    </button>
  );
}

interface SkipControlButtonProps extends ControlButtonProps {
  direction: "previous" | "next";
}

export function SkipControlButton({
  direction,
  label,
  onClick,
  buttonSize,
  iconSize,
  className,
}: SkipControlButtonProps) {
  const reduceMotion = useReducedMotion();
  const controls = useAnimationControls();
  const shift = direction === "previous" ? -4 : 4;

  const handleClick = () => {
    onClick();
    if (reduceMotion) return;
    void controls.start({
      x: [0, shift, -shift * 0.24, 0],
      scaleX: [1, 0.72, 1.08, 1],
      opacity: [1, 0.64, 1, 1],
      transition: { duration: 0.28, times: [0, 0.34, 0.72, 1], ease: MOTION_EASE },
    });
  };

  return (
    <button
      type="button"
      aria-label={label}
      onClick={handleClick}
      className={cn(CONTROL_BUTTON, "text-tx", className)}
      style={{ width: buttonSize, height: buttonSize }}
    >
      <motion.span
        aria-hidden="true"
        animate={controls}
        whileHover={reduceMotion ? undefined : { scale: 1.08 }}
        className="inline-grid place-items-center"
        style={{ width: iconSize, height: iconSize }}
      >
        <svg width={iconSize} height={iconSize} viewBox="0 0 24 24" fill="currentColor">
          <g transform={direction === "previous" ? "translate(24 0) scale(-1 1)" : undefined}>
            <path d="M7.5 6.08c-.69-.43-1.58.07-1.58.88v10.08c0 .81.89 1.31 1.58.88l7.83-5.04a1.04 1.04 0 0 0 0-1.76z" />
            <rect x="17.05" y="5.15" width="2.2" height="13.7" rx="1.1" />
          </g>
        </svg>
      </motion.span>
    </button>
  );
}

interface RepeatControlButtonProps extends ControlButtonProps {
  mode: RepeatMode;
}

export function RepeatControlIcon({ mode, size }: { mode: RepeatMode; size: number }) {
  const reduceMotion = useReducedMotion();
  const previousMode = useRef(mode);
  const arrowControls = useAnimationControls();

  useEffect(() => {
    if (previousMode.current === mode) return;
    previousMode.current = mode;
    if (reduceMotion) return;

    void arrowControls.start({
      rotate: [0, 180],
      scale: [1, 0.88, 1],
      transition: { duration: 0.32, times: [0, 0.48, 1], ease: MOTION_EASE },
    });
  }, [arrowControls, mode, reduceMotion]);

  return (
    <motion.span
      aria-hidden="true"
      whileHover={reduceMotion ? undefined : { scale: 1.08 }}
      className="inline-grid shrink-0 place-items-center"
      style={{ width: size, height: size }}
    >
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <motion.g animate={arrowControls} style={{ transformOrigin: "12px 12px" }}>
          <path d="M4 9a7.7 7.7 0 0 1 12.8-2.8L20 9.4M20 5.2v4.2h-4.2" />
          <path d="M20 15a7.7 7.7 0 0 1-12.8 2.8L4 14.6M4 18.8v-4.2h4.2" />
        </motion.g>
        <AnimatePresence initial={false}>
          {mode === "one" && (
            <motion.path
              key="repeat-one"
              d="m10.8 10.2 1.4-1.1v5.8"
              initial={reduceMotion ? { opacity: 0 } : { opacity: 0, scale: 0.5 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={reduceMotion ? { opacity: 0 } : { opacity: 0, scale: 1.35 }}
              transition={
                reduceMotion
                  ? { duration: 0.01 }
                  : { type: "spring", stiffness: 520, damping: 25, mass: 0.55 }
              }
              style={{ transformOrigin: "12px 12px" }}
            />
          )}
        </AnimatePresence>
      </svg>
    </motion.span>
  );
}

export function RepeatControlButton({
  mode,
  label,
  onClick,
  buttonSize,
  iconSize,
  className,
}: RepeatControlButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      data-repeat-mode={mode}
      onClick={onClick}
      className={cn(
        CONTROL_BUTTON,
        mode === "off" ? "text-tx2 hover:text-tx" : "text-ac",
        className,
      )}
      style={{ width: buttonSize, height: buttonSize }}
    >
      <RepeatControlIcon mode={mode} size={iconSize} />
    </button>
  );
}
