import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { Button, Card } from "@lanpath/ui";
import { backupExport, backupGeneratePassword, toErrorMessage, toastError, toastSuccess } from "@lanpath/client-core";
import type { BackupAppearance } from "@lanpath/client-core";
import { SettingsInput } from "../fields";

export function ExportCard({ appearance }: { appearance: BackupAppearance }) {
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [visible, setVisible] = useState(false);
  const generate = useMutation({
    mutationFn: backupGeneratePassword,
    onSuccess: (value) => {
      setPassword(value);
      setConfirm(value);
      setVisible(true);
    },
    onError: (e) => toastError(toErrorMessage(e)),
  });
  const exportFile = useMutation({
    mutationFn: () => backupExport(password, appearance),
    onSuccess: () => {
      setVisible(false);
      toastSuccess("加密备份已导出");
    },
  });
  const copy = useMutation({
    mutationFn: () => navigator.clipboard.writeText(password),
    onSuccess: () => toastSuccess("密码已复制，请单独妥善保存"),
    onError: () => toastError("复制失败，请选中密码手动复制"),
  });
  const valid =
    [...password].length >= 12 && new TextEncoder().encode(password).byteLength <= 1024 && password === confirm;
  const busy = exportFile.isPending || generate.isPending;
  return (
    <Card>
      <Card.Header>
        <Card.Title>导出加密备份</Card.Title>
        <Card.Description>订阅、覆写模板、分流配置、脚本及外观偏好，打包为 .zip.age 文件。</Card.Description>
      </Card.Header>
      <Card.Content className="flex flex-col gap-4">
        <p className="text-sm text-muted">密码不会写入备份，也无法找回。推荐生成随机密码，与备份文件分开保存。</p>
        <SettingsInput
          id="backup-password"
          label="备份密码"
          type={visible ? "text" : "password"}
          value={password}
          onChange={setPassword}
          disabled={busy}
          hint="至少 12 个字符，支持较长口令"
        />
        <div className="flex flex-wrap gap-2">
          <Button variant="secondary" size="sm" isDisabled={busy} onPress={() => generate.mutate()}>
            生成随机密码
          </Button>
          <Button variant="tertiary" size="sm" isDisabled={!password || busy} onPress={() => copy.mutate()}>
            复制密码
          </Button>
          <Button variant="tertiary" size="sm" isDisabled={busy} onPress={() => setVisible(!visible)}>
            {visible ? "隐藏密码" : "显示密码"}
          </Button>
        </div>
        <SettingsInput
          id="backup-confirm"
          label="再次输入密码"
          type="password"
          value={confirm}
          onChange={setConfirm}
          disabled={busy}
          error={confirm && password !== confirm ? "两次密码不一致" : null}
        />
        <Button
          variant="primary"
          isDisabled={!valid || busy}
          isPending={exportFile.isPending}
          onPress={() => exportFile.mutate()}
        >
          加密并导出
        </Button>
        {exportFile.isPending && <p className="text-sm text-muted">正在压缩并加密，请勿关闭应用…</p>}
        {exportFile.data && <output className="break-all text-sm text-success">已保存至 {exportFile.data}</output>}
        {exportFile.error && <p className="text-sm text-danger">{toErrorMessage(exportFile.error)}</p>}
      </Card.Content>
    </Card>
  );
}
