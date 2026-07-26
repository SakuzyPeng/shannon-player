import { Icon } from "@/components/common/Icon";
import { useT } from "@/i18n";
import type { MessageKey } from "@/i18n/messages";

/**
 * 详情页找不到内容时的兜底页。
 *
 * 三个详情页原本都是 `if (!x) return null` ——条件一旦成立，整个组件不渲染，
 * 用户看到的是一片空白**且没有返回按钮**，只能靠侧边导航跳走，退不回原来的列表。
 * 这不是假想：重新扫描后专辑 ID 会变（它是聚合派生的），删掉歌单、曲目全被移出
 * 曲库也都会走到这里。
 *
 * 所以空态要么给内容，要么至少留下退路——这里选后者：说明发生了什么，并把返回
 * 按钮摆在与正常详情页面包屑相同的位置上。
 */
export function DetailNotFound({
  backLabel,
  onBack,
}: {
  /** 返回目标的名称，与正常详情页的面包屑一致（如「专辑」「歌单」）。 */
  backLabel: MessageKey;
  onBack: () => void;
}) {
  const { t } = useT();
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      {/* 位置与正常详情页的面包屑对齐，用户不必重新找返回在哪 */}
      <div data-tauri-drag-region className="flex flex-none items-center px-10 pt-[22px]">
        <button
          onClick={onBack}
          className="flex cursor-pointer items-center gap-1.5 rounded-full py-[5px] pl-2 pr-3 text-[12.5px] text-tx2 transition-colors hover:bg-hv hover:text-tx"
        >
          <Icon name="chevronLeft" size={13} strokeWidth={2} />
          {t(backLabel)}
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3.5 px-10 pb-[120px] text-center">
        <div className="grid size-16 place-items-center rounded-full border border-bd bg-sb text-tx2">
          <Icon name="search" size={24} strokeWidth={1.7} />
        </div>
        <div className="font-serif text-lg font-semibold text-tx">{t("detail.missingTitle")}</div>
        <div className="max-w-[320px] text-[13px] leading-[1.6] text-tx2">
          {t("detail.missingBody")}
        </div>
      </div>
    </div>
  );
}
