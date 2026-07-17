import type { CSSProperties } from "react";
import { cn } from "@/lib/cn";

interface VolumeSliderProps {
  value: number;
  muted: boolean;
  label: string;
  onChange: (value: number) => void;
  className?: string;
}

export function VolumeSlider({
  value,
  muted,
  label,
  onChange,
  className,
}: VolumeSliderProps) {
  const displayValue = muted ? 0 : Math.min(1, Math.max(0, value));
  const percentage = Math.round(displayValue * 100);
  const style = { "--volume-fill": `${percentage}%` } as CSSProperties;

  return (
    <input
      type="range"
      min="0"
      max="1"
      step="0.01"
      value={displayValue}
      aria-label={label}
      aria-valuetext={`${percentage}%`}
      onChange={(event) => onChange(Number(event.currentTarget.value))}
      className={cn("volume-slider shrink-0", className)}
      style={style}
    />
  );
}
