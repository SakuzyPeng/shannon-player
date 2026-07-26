import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Icon } from "@/components/common/Icon";

/**
 * 「显示全部 / 收起」切换钮。
 *
 * 两个刻意的选择：
 *
 * 1. **用 `chevronDown` 旋转，而不是左右箭头**。左右箭头在界面语汇里意味着「去往
 *    别处」（面包屑、进入下一级），而这里是**就地展开**，什么都没跳转。旋转本身
 *    也是天然的过渡动作，一个 CSS transition 就够，不必额外做动画。
 * 2. **文字交叉淡入 + 宽度平滑**。「显示全部 108 首歌曲」与「收起」宽度相差很大，
 *    直接替换会让按钮猛地缩短。`layout` 会给按钮引入 layout 动画——就是专辑封面
 *    弹跳的那个机制，但按钮是孤立元素，牵连不到别人。
 *
 * 遵从「减少动态效果」偏好：开启时直接切换，不做位移与淡入。
 */
export function ExpandToggle({
  open,
  onToggle,
  openLabel,
  closeLabel,
}: {
  open: boolean;
  onToggle: () => void;
  /** 收起态的文案，如「显示全部 108 首歌曲」。 */
  openLabel: string;
  /** 展开态的文案，如「收起」。 */
  closeLabel: string;
}) {
  const reduce = useReducedMotion();
  const label = open ? closeLabel : openLabel;
  return (
    <motion.button
      layout={!reduce}
      transition={{ type: "spring", stiffness: 420, damping: 34 }}
      onClick={onToggle}
      aria-expanded={open}
      className="flex flex-none cursor-pointer items-center gap-1 whitespace-nowrap rounded-full px-3 py-1.5 text-[12.5px] text-tx2 transition-colors hover:bg-hv hover:text-tx"
    >
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span
          key={label}
          initial={reduce ? false : { opacity: 0, y: -4 }}
          animate={{ opacity: 1, y: 0 }}
          exit={reduce ? undefined : { opacity: 0, y: 4 }}
          transition={{ duration: 0.16 }}
        >
          {label}
        </motion.span>
      </AnimatePresence>
      <Icon
        name="chevronDown"
        size={12}
        strokeWidth={2}
        className="flex-none"
        style={{
          transform: open ? "rotate(180deg)" : undefined,
          transition: reduce ? undefined : "transform 0.25s var(--ease-spring)",
        }}
      />
    </motion.button>
  );
}
