import { useEffect, useRef } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Button, IS_MOBILE, Modal } from "@lanpath/ui";
import {
  BACKUP_RESTORED_KEY,
  BACKUP_PENDING_KEY,
  backupPending,
  backupRestored,
  backupAcknowledge,
  backupRestart,
  toastError,
  toastSuccess,
  toErrorMessage,
} from "@lanpath/client-core";
import { useThemePreference } from "../theme";
import { ACCENT_OPTIONS } from "../appearance";

export function BackupLifecycle() {
  const pending = useQuery({ queryKey: BACKUP_PENDING_KEY, queryFn: backupPending });
  const restored = useQuery({
    queryKey: BACKUP_RESTORED_KEY,
    queryFn: backupRestored,
    staleTime: Infinity,
    retry: false,
  });
  const theme = useThemePreference();
  const applied = useRef(false);
  useEffect(() => {
    if (applied.current) return;
    if (restored.error) {
      applied.current = true;
      toastError(toErrorMessage(restored.error));
      void backupAcknowledge().catch(() => undefined);
      return;
    }
    if (!restored.data) return;
    applied.current = true;
    const appearance = restored.data.appearance;
    if (appearance) {
      theme.setTheme(appearance.theme);
      theme.setPureBlack(appearance.pure_black);
      const accent = ACCENT_OPTIONS.find((option) => option.value === appearance.accent);
      if (accent) theme.setAccent(accent.value);
    }
    void backupAcknowledge().catch((e) => toastError(toErrorMessage(e)));
    toastSuccess("备份配置已恢复，请按需重新开启代理");
  }, [restored.data, restored.error, theme]);
  const restart = useMutation({ mutationFn: backupRestart, onError: (e) => toastError(toErrorMessage(e)) });
  return (
    <Modal.Backdrop isOpen={Boolean(pending.data)} onOpenChange={() => undefined}>
      <Modal.Container>
        <Modal.Dialog>
          <Modal.Header>
            <Modal.Heading>恢复已准备就绪</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <p className="text-sm">代理已停止。重启后将替换配置，恢复前的加密副本已保存在本机，密码与本次导入相同。</p>
            <p className="mt-3 text-sm text-muted">
              {IS_MOBILE
                ? "点击下方按钮关闭应用，再从桌面重新打开，即可完成恢复。"
                : "重启后不会自动开启系统代理或 TUN，请确认配置后手动启动。"}
            </p>
          </Modal.Body>
          <Modal.Footer>
            <Button variant="primary" isPending={restart.isPending} onPress={() => restart.mutate()}>
              {IS_MOBILE ? "关闭应用以完成恢复" : "立即重启并恢复"}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
