import { SubPageShell } from "../../../components/PageShell";
import { useThemePreference } from "../../../theme";
import { ExportCard } from "./ExportCard";
import { ImportCard } from "./ImportCard";

export default function BackupPage() {
  const { theme, accent, pureBlack } = useThemePreference();
  const appearance = { theme, accent, pure_black: pureBlack };
  return (
    <SubPageShell title="备份与恢复" backTo="/settings">
      <p className="text-sm text-muted">
        在设备间迁移配置。核心程序、日志、流量历史、证书私钥和系统授权不包含在备份中。
      </p>
      <ExportCard appearance={appearance} />
      <ImportCard appearance={appearance} />
    </SubPageShell>
  );
}
