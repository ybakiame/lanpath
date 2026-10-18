import type { ReactNode } from "react";
import { Signal } from "lucide-react";
import { RULE_MODE_LABELS } from "../ruleModes";

interface StatusCardProps {
  running: boolean;
  starting?: boolean;
  ruleMode: string;
  missingRuleSets?: string[];
  action: ReactNode;
}

/** 真实核心状态为主指标；装饰路径不表示流量历史或连接质量。 */
export function StatusCard({ running, starting = false, ruleMode, missingRuleSets = [], action }: StatusCardProps) {
  return (
    <section className="lanpath-status-hero flex flex-col gap-6 p-6" aria-label="运行状态">
      <div className="flex items-center justify-between gap-3">
        <span className="lanpath-hero-muted text-sm">连接状态</span>
        <span className="lanpath-hero-muted flex items-center gap-2 text-xs">
          <Signal className="size-4" aria-hidden="true" />
          {RULE_MODE_LABELS[ruleMode] ?? ruleMode}
        </span>
      </div>
      <div className="flex flex-col gap-2" aria-live="polite">
        <h2 className="text-4xl font-semibold tracking-tight">
          {starting ? "正在连接" : running ? "已连接" : "准备连接"}
        </h2>
        <p className="lanpath-hero-muted text-sm">
          {starting
            ? "正在准备配置与网络资源"
            : running
              ? "代理运行中，沿你的路径出发"
              : "代理已停止，选择订阅后即可出发"}
        </p>
      </div>
      {action}
      {running && missingRuleSets.length > 0 && (
        <p className="lanpath-hero-muted text-xs leading-relaxed">
          分流规则集未全部就绪（{missingRuleSets.join("、")}），已降级运行；补齐后自动恢复完整分流。
        </p>
      )}
    </section>
  );
}
