import { Play, Square } from "lucide-react";
import { Button } from "@lanpath/ui";
import { Spinner } from "@lanpath/ui";

interface StartStopButtonProps {
  /** 核心是否运行中：决定启停图标与文案。 */
  running: boolean;
  /** 启动链路 busy（`startMutation.isPending || vpnAuthMutation.isPending`）。 */
  starting: boolean;
  /** 停止请求进行中（`stopMutation.isPending`）。 */
  stopping: boolean;
  /** 是否存在生效订阅；无生效订阅且未运行时禁用启动。 */
  canStart: boolean;
  /** 点击启动（复用 Dashboard `handleStart`）。 */
  onStart: () => void;
  /** 点击停止（复用 Dashboard `handleStop`）。 */
  onStop: () => void;
}

/** 状态卡内的主操作，沿用 Dashboard 的启停与 VPN 授权链路。 */
export function StartStopButton({ running, starting, stopping, canStart, onStart, onStop }: StartStopButtonProps) {
  const pending = running ? stopping : starting;
  const disabled = running ? starting : !canStart || starting || stopping;

  return (
    <Button
      aria-label={running ? "停止代理" : "启动代理"}
      aria-busy={pending}
      onPress={() => {
        if (!disabled) void (running ? onStop() : onStart());
      }}
      disabled={disabled || pending}
      className="lanpath-status-action min-h-14 w-full gap-2 rounded-full font-semibold"
    >
      {pending ? (
        <Spinner size="sm" color="current" aria-hidden="true" />
      ) : running ? (
        <Square className="size-6" aria-hidden="true" />
      ) : (
        <Play className="size-6" aria-hidden="true" />
      )}
      {pending ? (running ? "正在停止…" : "正在连接…") : running ? "停止代理" : "启动代理"}
    </Button>
  );
}
