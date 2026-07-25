import { Fragment } from "react";

/** 元信息统一使用的分隔符（与各语言字典一致）。 */
const SEP = " · ";

/**
 * 头部元信息行（「3 首 · 2 张专辑 · 4 位歌手」这类一行摘要）。
 *
 * 每个片段包成不可拆的整体，折行只能落在分隔符处——避免出现「1」与
 * 「playlists」被拆到两行的孤字断行。配合标题列 flex-none（正常宽度下
 * 根本不折行），这里只是极窄场景的兜底。
 */
export function MetaLine({ text, className }: { text: string; className?: string }) {
  const parts = text.split(SEP);
  if (parts.length === 1) return <span className={className}>{text}</span>;
  return (
    <span className={className}>
      {parts.map((part, i) => (
        <Fragment key={i}>
          {i > 0 && SEP}
          <span className="whitespace-nowrap">{part}</span>
        </Fragment>
      ))}
    </span>
  );
}
