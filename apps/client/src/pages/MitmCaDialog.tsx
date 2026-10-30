import { AlertDialog, Button } from "@lanpath/ui";

export interface MitmCaAuthorization {
  action: "install" | "remove";
  sha256: string;
  scope: string;
}

interface MitmCaDialogProps {
  authorization: MitmCaAuthorization | null;
  onCancel: () => void;
  onConfirm: (authorization: MitmCaAuthorization) => void;
}

export function MitmCaDialog({ authorization, onCancel, onConfirm }: MitmCaDialogProps) {
  const installing = authorization?.action === "install";
  return (
    <AlertDialog.Backdrop
      isOpen={authorization !== null}
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <AlertDialog.Container size="sm">
        <AlertDialog.Dialog>
          <AlertDialog.CloseTrigger />
          <AlertDialog.Header>
            <AlertDialog.Heading>
              {installing ? "安装并信任 MITM CA？" : "移除本应用拥有的 CA 信任？"}
            </AlertDialog.Heading>
          </AlertDialog.Header>
          <AlertDialog.Body>
            <p className="break-all font-mono text-xs">SHA-256：{authorization?.sha256}</p>
            <p>范围：{authorization?.scope === "current_user_root" ? "Windows CurrentUser Root" : "系统信任库"}</p>
            <p>
              {installing
                ? "信任后可解密白名单 HTTPS 流量。停核、退出或重启不会撤销信任；不会自动启用系统代理。"
                : "仅移除本应用新增且原始所有权仍匹配的证书。既有根和所有权已变的证书保持不动。"}
            </p>
          </AlertDialog.Body>
          <AlertDialog.Footer>
            <Button slot="close" variant="tertiary" onPress={onCancel}>
              取消
            </Button>
            <Button
              slot="close"
              variant={installing ? "primary" : "danger"}
              onPress={() => {
                if (authorization) onConfirm(authorization);
              }}
            >
              {installing ? "安装并信任" : "移除拥有的信任"}
            </Button>
          </AlertDialog.Footer>
        </AlertDialog.Dialog>
      </AlertDialog.Container>
    </AlertDialog.Backdrop>
  );
}
