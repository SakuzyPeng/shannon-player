import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { EqBars } from "./EqBars";
import { Icon } from "./Icon";

interface TrackIndicatorProps {
  number: number | string;
  active: boolean;
  playing: boolean;
  showGripOnHover?: boolean;
  gripTitle?: string;
}

/** 歌曲行左侧的稳定槽位：序号、均衡器和拖拽柄均在此处交叉淡化。 */
export function TrackIndicator({
  number,
  active,
  playing,
  showGripOnHover = false,
  gripTitle,
}: TrackIndicatorProps) {
  const reduceMotion = useReducedMotion();
  const transition = reduceMotion
    ? { duration: 0.01 }
    : { duration: 0.16, ease: [0.22, 1, 0.36, 1] as const };

  return (
    <span className="relative grid h-7 w-8 shrink-0 place-items-center">
      <AnimatePresence initial={false}>
        {active ? (
          <motion.span
            key="equalizer"
            className="absolute inset-0 grid place-items-center"
            initial={{ opacity: 0, scale: reduceMotion ? 1 : 0.78 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: reduceMotion ? 1 : 1.12 }}
            transition={transition}
          >
            <EqBars playing={playing} />
          </motion.span>
        ) : (
          <motion.span
            key="number"
            className="absolute inset-0 grid place-items-center"
            initial={{ opacity: 0, scale: reduceMotion ? 1 : 0.88 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: reduceMotion ? 1 : 1.08 }}
            transition={transition}
          >
            <span
              className={
                showGripOnHover
                  ? "transition-opacity duration-150 group-hover/row:opacity-0"
                  : undefined
              }
            >
              {number}
            </span>
            {showGripOnHover && (
              <span
                className="absolute inset-0 grid place-items-center opacity-0 transition-opacity duration-150 group-hover/row:opacity-100"
                title={gripTitle}
              >
                <Icon name="grip" size={16} />
              </span>
            )}
          </motion.span>
        )}
      </AnimatePresence>
    </span>
  );
}
