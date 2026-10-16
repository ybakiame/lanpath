import { BellRing, Code, Globe, Info, Palette, Network, Archive } from "lucide-react";
import { IS_MOBILE } from "@lanpath/ui";
import DesktopSettingsSections from "./desktop/DesktopSettingsSections";
import { useNavigate } from "react-router-dom";
import { EntryLinkList } from "../../components/mobile/EntryLinkCard";
import { PageShell } from "../../components/PageShell";
import { useThemePreference } from "../../theme";
import { ACCENT_OPTIONS } from "../../appearance";

/** 设置主页通过主题入口进入二级页；其余配置按平台能力呈现。 */
export default function Settings() {
  const navigate = useNavigate();
  const { theme, accent, pureBlack, resolvedTheme } = useThemePreference();
  const themeLabel = { system: "跟随系统", light: "浅色", dark: "深色" }[theme];
  const accentLabel = ACCENT_OPTIONS.find((option) => option.value === accent)?.label;

  return (
    <PageShell>
      <div>
        <h1 className={IS_MOBILE ? "text-3xl font-semibold tracking-tight" : "text-xl font-semibold"}>设置</h1>
        {!IS_MOBILE && <p className="text-sm text-muted">客户端外观、网络请求与核心运行配置</p>}
      </div>

      {IS_MOBILE && <p className="-mt-3 text-sm text-muted">你的外观偏好与连接工具</p>}

      <EntryLinkList
        entries={[
          {
            icon: <Palette className="size-5" aria-hidden="true" />,
            title: "主题",
            description: `${themeLabel}${resolvedTheme === "dark" && pureBlack ? "（纯黑背景）" : ""} · ${accentLabel}`,
            onPress: () => navigate("/settings/theme"),
          },
          {
            icon: <Network className="size-5" aria-hidden="true" />,
            title: "网络请求",
            description: "订阅与远程资源的代理方式",
            onPress: () => navigate("/settings/requests"),
          },
          {
            icon: <Archive className="size-5" aria-hidden="true" />,
            title: "备份与恢复",
            description: "加密导出、跨设备导入配置",
            onPress: () => navigate("/settings/backup"),
          },
        ]}
      />

      {IS_MOBILE ? (
        <EntryLinkList
          entries={[
            {
              icon: <BellRing className="size-5" aria-hidden="true" />,
              title: "VPN 通知",
              description: "通知栏显示流量与节点选择",
              onPress: () => navigate("/settings/vpn-notify"),
            },
            {
              icon: <Globe className="size-5" aria-hidden="true" />,
              title: "GitHub 访问",
              description: "代理前缀加速",
              onPress: () => navigate("/settings/github"),
            },
            {
              icon: <Code className="size-5" aria-hidden="true" />,
              title: "开发者工具",
              description: "日志与当前配置",
              onPress: () => navigate("/settings/dev-tools"),
            },
            {
              icon: <Info className="size-5" aria-hidden="true" />,
              title: "关于",
              description: "应用与核心信息",
              onPress: () => navigate("/settings/about"),
            },
          ]}
        />
      ) : (
        <DesktopSettingsSections />
      )}
    </PageShell>
  );
}
