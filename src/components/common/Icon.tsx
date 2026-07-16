import type { CSSProperties } from "react";

export type IconName =
  | "albums"
  | "songs"
  | "artists"
  | "favorites"
  | "settings"
  | "globe"
  | "sun"
  | "moon"
  | "monitor"
  | "search"
  | "shuffle"
  | "repeat"
  | "queue"
  | "addPlaylist"
  | "check"
  | "prev"
  | "next"
  | "play"
  | "pause"
  | "heart"
  | "volume"
  | "chevronLeft"
  | "chevronRight"
  | "chevronDown"
  | "close"
  | "grip"
  | "more";

/** 描边类图标：单个 d（含多段子路径）。 */
const STROKE: Partial<Record<IconName, string>> = {
  albums: "M4 4h7v7H4z M13 4h7v7h-7z M4 13h7v7H4z M13 13h7v7h-7z",
  songs:
    "M9 17V5l11-2v12 M9 17a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0z M20 15a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0z",
  artists:
    "M12 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z M5 20v-.5A6.5 6.5 0 0 1 11.5 13h1A6.5 6.5 0 0 1 19 19.5v.5",
  favorites:
    "M12 20.5C7.2 16.6 4 13.6 4 10.2 4 7.9 5.8 6 8.1 6c1.4 0 2.8.7 3.9 2.2C13.1 6.7 14.5 6 15.9 6 18.2 6 20 7.9 20 10.2c0 3.4-3.2 6.4-8 10.3z",
  settings:
    "M6 4v4 M6 12v8 M12 4v10 M12 18v2 M18 4v2 M18 10v10 M4 8h4 M10 14h4 M16 6h4",
  globe:
    "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z M3 12h18 M12 3a15 15 0 0 1 0 18 M12 3a15 15 0 0 0 0 18",
  sun:
    "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10z M12 2v2 M12 20v2 M4.9 4.9l1.4 1.4 M17.7 17.7l1.4 1.4 M2 12h2 M20 12h2 M4.9 19.1l1.4-1.4 M17.7 6.3l1.4-1.4",
  moon: "M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z",
  monitor: "M4 5h16v11H4z M9 20h6 M12 16v4",
  shuffle: "M3 6h4l10 11h4 M17 3l4 3-4 3 M3 17h4 M14 8l3-2 M17 21l4-3-4-3",
  repeat: "M4 12a8 8 0 0 1 14-5l2 2 M20 12a8 8 0 0 1-14 5l-2-2 M20 4v5h-5 M4 20v-5h5",
  queue: "M4 6h16 M4 12h16 M4 18h10",
  addPlaylist: "M12 4v5 M9 6.5 12 4l3 2.5 M8 13a4 4 0 1 0 8 0 4 4 0 0 0-8 0z M4 20h16",
  check: "M5 12l5 5 9-10",
  chevronLeft: "M15 6l-6 6 6 6",
  chevronRight: "M9 6l6 6-6 6",
  chevronDown: "M6 9l6 6 6-6",
  close: "M6 6l12 12 M18 6L6 18",
};

export interface IconProps {
  name: IconName;
  size?: number;
  strokeWidth?: number;
  className?: string;
  style?: CSSProperties;
}

export function Icon({ name, size = 20, strokeWidth = 1.7, className, style }: IconProps) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    className,
    style,
  } as const;

  // 描边类
  if (STROKE[name]) {
    return (
      <svg
        {...common}
        fill="none"
        stroke="currentColor"
        strokeWidth={strokeWidth}
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d={STROKE[name]} />
      </svg>
    );
  }

  // 填充 / 复合类
  switch (name) {
    case "search":
      return (
        <svg {...common} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
          <circle cx="10.5" cy="10.5" r="7" />
          <path d="M16 16l5 5" />
        </svg>
      );
    case "play":
      return (
        <svg {...common} fill="currentColor" style={{ marginLeft: 2, ...style }}>
          <path d="M8 5v14l11-7z" />
        </svg>
      );
    case "pause":
      return (
        <svg {...common} fill="currentColor">
          <rect x="6" y="5" width="4" height="14" rx="1.2" />
          <rect x="14" y="5" width="4" height="14" rx="1.2" />
        </svg>
      );
    case "prev":
      return (
        <svg {...common} fill="currentColor">
          <path d="M18 5v14l-11-7z" />
          <rect x="5" y="5" width="2" height="14" rx="1" />
        </svg>
      );
    case "next":
      return (
        <svg {...common} fill="currentColor">
          <path d="M6 5v14l11-7z" />
          <rect x="17" y="5" width="2" height="14" rx="1" />
        </svg>
      );
    case "heart":
      return (
        <svg {...common} fill="currentColor">
          <path d="M12 20.5C7.2 16.6 4 13.6 4 10.2 4 7.9 5.8 6 8.1 6c1.4 0 2.8.7 3.9 2.2C13.1 6.7 14.5 6 15.9 6 18.2 6 20 7.9 20 10.2c0 3.4-3.2 6.4-8 10.3z" />
        </svg>
      );
    case "more":
      return (
        <svg {...common} fill="currentColor">
          <circle cx="5" cy="12" r="1.7" />
          <circle cx="12" cy="12" r="1.7" />
          <circle cx="19" cy="12" r="1.7" />
        </svg>
      );
    case "grip":
      return (
        <svg {...common} fill="currentColor">
          <circle cx="9" cy="5" r="1.6" />
          <circle cx="15" cy="5" r="1.6" />
          <circle cx="9" cy="12" r="1.6" />
          <circle cx="15" cy="12" r="1.6" />
          <circle cx="9" cy="19" r="1.6" />
          <circle cx="15" cy="19" r="1.6" />
        </svg>
      );
    case "volume":
      return (
        <svg
          {...common}
          fill="none"
          stroke="currentColor"
          strokeWidth={1.8}
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M11 5 6 9H3v6h3l5 4z" fill="currentColor" stroke="none" />
          <path d="M15.5 9.5a4 4 0 0 1 0 5.5 M18 7a8 8 0 0 1 0 10" />
        </svg>
      );
    default:
      return null;
  }
}
