import { useEffect } from "react";
import { usePlayerStore } from "@/store/player";
import { useUiStore } from "@/store/ui";

// 模块级而不是 effect 级：React StrictMode 会重挂载 effect，版本若跟着归零，第一次挂载
// 的迟到回执就可能与第二次的新请求撞号。
// 以时间戳划出前端生命周期：开发时重载 WebView 但复用 Tauri 引擎，新页面也必须从比
// 旧页面更大的版本继续。乘 1000 后仍在 JS 安全整数范围内，并给同一毫秒内的选择留余量。
let deviceRevision = Date.now() * 1000;
const nextDeviceRevision = () => ++deviceRevision;

/**
 * 把「用户选了哪台输出端点」持续告诉引擎。
 *
 * 与 `useSyncNext`、`useLoudnessQueue` 同一套路：**选择的权威在前端，落实交给后端**。
 * 做成订阅而不是在设置页的点击处直接调，理由也相同——将来任何地方（快捷键、播放条、
 * 系统菜单）改了这个偏好都会自动生效，不需要谁记得补一行。
 *
 * ## 为什么要先拿到设备列表才敢下发
 *
 * 存下来的那台设备可能已经不在了：拔了、关了，或者这份设置来自另一台机器。直接下发的
 * 话，引擎会如实拒绝并回一条 `deviceRejected`——于是用户每次启动都看见一句「设备不可用」
 * 的提示，而他什么也没做。所以这里先对着真实列表核对：在，就下发；不在，就**保留偏好
 * 但不下发**，本次跟随系统默认。
 *
 * 偏好不清掉是刻意的。清掉最省事，代价是「晚上拔了 DAC，第二天插回来还得重选一次」。
 * 保留则要求界面把话说清楚——设置页会把这台显示成「（当前不可用）」，用户既知道偏好
 * 还在，也理解为什么此刻声音走的是别处。
 *
 * ## 并发选择按 latest-wins 对账
 *
 * `player_set_device` 一入队就返回，真正结果随后才由事件告知。用户快速点 A → B → C 时，
 * 三次预检可能交错完成，所以每次请求都带单调递增的 revision；引擎只接纳最新版本，store
 * 也只接纳当前版本的回执。被拒后的真实偏好由引擎随事件带回并在 store 里回滚，不能用
 * 一格本地点击历史猜——那一格在并发下可能正好是另一条尚未落定的请求。
 */
export function useSyncDevice() {
  useEffect(() => {
    let cancelled = false;
    // 已经下发的目标。避免同一选择重复拆流；`undefined` 表示还从未下发。
    let appliedId: string | null | undefined;

    const apply = async () => {
      const player = usePlayerStore.getState();
      let devices = player.devices;
      if (devices.length === 0) {
        await player.refreshDevices();
        if (cancelled) return;
        devices = usePlayerStore.getState().devices;
      }
      const preferred = useUiStore.getState().outputDevice;
      // 认不出来的偏好一律当作「跟随系统默认」下发。设备列表拿不到时（枚举失败）
      // 同样如此：宁可退回默认，也不拿一个没核对过的标识去撞。
      const target = preferred && devices.some((d) => d.id === preferred.id) ? preferred : null;
      const targetId = target?.id ?? null;
      if (targetId === appliedId) return;

      appliedId = targetId;
      const revision = nextDeviceRevision();
      try {
        await usePlayerStore.getState().setDeviceOnEngine(targetId, revision);
      } catch (error) {
        // 命令根本没发出去时没有后端回执可对账。只允许最新请求恢复为“未下发”，
        // 让下一次用户动作可以重试；旧请求的失败不能动更新选择。
        console.error("选定输出端点失败", error);
        if (usePlayerStore.getState().deviceRevision === revision) appliedId = undefined;
      }
    };

    void apply();

    const stopUi = useUiStore.subscribe((state, prev) => {
      if (state.outputDevice !== prev.outputDevice) void apply();
    });

    return () => {
      cancelled = true;
      stopUi();
    };
  }, []);
}
