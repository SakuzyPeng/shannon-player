/** 当前播放行的跳动均衡器：3 根 3px 柱，相位差 0.25s，暂停时定格。 */
export function EqBars({ playing }: { playing: boolean }) {
  return (
    <div className="flex h-[15px] items-end gap-[2.5px] pl-[3px]">
      {[0, 0.25, 0.5].map((delay) => (
        <div
          key={delay}
          className="h-full w-[3px] origin-bottom rounded-[1.5px] bg-ac"
          style={{
            animation: `eq 0.9s ease-in-out ${delay}s infinite`,
            animationPlayState: playing ? "running" : "paused",
          }}
        />
      ))}
    </div>
  );
}
