import { motion, useReducedMotion } from "framer-motion";

interface PlayPauseIconProps {
  playing: boolean;
  size?: number;
  className?: string;
}

/* 播放三角拆成上下两片，保持与暂停双柱相同的路径拓扑，切换时可直接形变。 */
const PLAY_TOP =
  "M8.72 5.2 C8.1 4.82 7.35 5.25 7.35 5.98 L7.35 11.72 C7.35 11.93 7.52 12.1 7.73 12.1 L18.28 12.1 C19.02 12.1 19.3 11.12 18.66 10.72 L9.08 5.18 C8.96 5.11 8.83 5.12 8.72 5.2 Z";
const PLAY_BOTTOM =
  "M7.73 11.9 C7.52 11.9 7.35 12.07 7.35 12.28 L7.35 18.02 C7.35 18.75 8.1 19.18 8.72 18.8 L18.66 13.28 C19.3 12.88 19.02 11.9 18.28 11.9 L8.12 11.9 C7.98 11.9 7.85 11.9 7.73 11.9 Z";
const PAUSE_LEFT =
  "M7.9 4.85 C7.05 4.85 6.5 5.4 6.5 6.25 L6.5 17.75 C6.5 18.6 7.05 19.15 7.9 19.15 L9.15 19.15 C10 19.15 10.55 18.6 10.55 17.75 L10.55 6.25 C10.55 5.4 10 4.85 9.15 4.85 Z";
const PAUSE_RIGHT =
  "M14.85 4.85 C14 4.85 13.45 5.4 13.45 6.25 L13.45 17.75 C13.45 18.6 14 19.15 14.85 19.15 L16.1 19.15 C16.95 19.15 17.5 18.6 17.5 17.75 L17.5 6.25 C17.5 5.4 16.95 4.85 16.1 4.85 Z";

const MORPH_EASE = [0.22, 1, 0.36, 1] as const;

/** 光学居中的圆角播放 / 暂停图标；两段路径在状态切换时连续形变。 */
export function PlayPauseIcon({ playing, size = 20, className }: PlayPauseIconProps) {
  const reduceMotion = useReducedMotion();
  const transition = reduceMotion
    ? { duration: 0.01 }
    : { duration: 0.22, ease: MORPH_EASE };

  return (
    <svg
      aria-hidden="true"
      data-play-state={playing ? "pause" : "play"}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="currentColor"
      className={className}
    >
      <motion.path
        initial={false}
        animate={{ d: playing ? PAUSE_LEFT : PLAY_TOP }}
        transition={transition}
      />
      <motion.path
        initial={false}
        animate={{ d: playing ? PAUSE_RIGHT : PLAY_BOTTOM }}
        transition={transition}
      />
    </svg>
  );
}
