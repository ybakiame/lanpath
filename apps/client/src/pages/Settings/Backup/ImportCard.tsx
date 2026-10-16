import { useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Button, Card } from "@lanpath/ui";
import {
  BACKUP_PENDING_KEY,
  backupPrepareRestore,
  backupPreview,
  backupPrevious,
  readBackupFile,
  toErrorMessage,
} from "@lanpath/client-core";
import type { BackupAppearance } from "@lanpath/client-core";
import { SettingsInput } from "../fields";

export function ImportCard({ appearance }: { appearance: BackupAppearance }) {
  const queryClient = useQueryClient();
  const input = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<{ name: string; content: string } | null>(null);
  const [password, setPassword] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const reset = () => {
    inspect.reset();
    restore.reset();
    setConfirmed(false);
  };
  const selectFile = useMutation({
    mutationFn: async (next: File) => ({ name: next.name, content: await readBackupFile(next) }),
    onMutate: () => {
      reset();
      setFile(null);
    },
    onSuccess: setFile,
  });
  const previous = useMutation({
    mutationFn: async () => {
      const content = await backupPrevious();
      if (!content) throw new Error("还没有恢复前备份");
      return { name: "最近一次恢复前备份（使用当次导入的密码）", content };
    },
    onMutate: () => {
      reset();
      setFile(null);
    },
    onSuccess: setFile,
  });
  const inspect = useMutation({
    mutationFn: () => backupPreview(file!.content, password),
  });
  const restore = useMutation({
    onError: () => {
      inspect.reset();
      setConfirmed(false);
    },
    mutationFn: () => backupPrepareRestore(inspect.data!.preview_id, appearance),
    onSuccess: async () => {
      setPassword("");
      setFile(null);
      reset();
      await queryClient.invalidateQueries({ queryKey: BACKUP_PENDING_KEY });
    },
  });
  const preview = inspect.data;
  const busy = selectFile.isPending || previous.isPending || inspect.isPending || restore.isPending;
  const error = inspect.error ?? restore.error ?? (!file ? (selectFile.error ?? previous.error) : null);
  return (
    <Card>
      <Card.Header>
        <Card.Title>导入并恢复</Card.Title>
        <Card.Description>先解密校验，再确认替换。恢复前自动保留本机加密副本。</Card.Description>
      </Card.Header>
      <Card.Content className="flex flex-col gap-4">
        <input
          ref={input}
          className="hidden"
          type="file"
          accept=".age"
          aria-label="选择加密备份"
          disabled={busy}
          onChange={(e) => {
            const next = e.target.files?.[0];
            if (next) selectFile.mutate(next);
            e.target.value = "";
          }}
        />
        <div className="flex flex-wrap gap-2">
          <Button variant="secondary" isDisabled={busy} onPress={() => input.current?.click()}>
            选择备份文件
          </Button>
          <Button variant="tertiary" isDisabled={busy} onPress={() => previous.mutate()}>
            使用恢复前备份
          </Button>
        </div>
        <p className="break-all text-sm text-muted">{file?.name ?? "支持 Lanpath .zip.age 备份，最大 16 MiB"}</p>
        <SettingsInput
          id="restore-password"
          label="解密密码"
          type="password"
          value={password}
          disabled={busy}
          onChange={(value) => {
            setPassword(value);
            reset();
          }}
        />
        <Button
          variant="secondary"
          isDisabled={!file || !password || busy}
          isPending={inspect.isPending}
          onPress={() => inspect.mutate()}
        >
          解密并检查
        </Button>
        {inspect.isPending && <p className="text-sm text-muted">正在解密并校验备份…</p>}
        {restore.isPending && <p className="text-sm text-muted">正在保存恢复前副本并准备恢复，请勿关闭应用…</p>}
        {preview && (
          <div className="flex flex-col gap-3 rounded-xl border border-border p-4">
            <p className="text-sm font-medium">
              备份来自 {preview.platform} · Lanpath {preview.app_version}
            </p>
            <p className="text-sm text-muted">
              {new Date(preview.created_at).toLocaleString()} · {preview.subscriptions} 个订阅 · {preview.profiles}{" "}
              个覆写模板 · {preview.files} 个文件
            </p>
            <p className="text-sm text-muted">
              替换配置，不合并订阅。代理会停止，重启后生效；系统代理、TUN 和 MITM
              需重新开启。本地文件引用需重新检查，仅导入你信任的备份。
            </p>
            <label className="flex min-h-11 items-start gap-3 text-sm">
              <input
                className="mt-1 size-4 shrink-0"
                type="checkbox"
                checked={confirmed}
                disabled={busy}
                onChange={(e) => setConfirmed(e.target.checked)}
              />
              我确认替换当前配置，并保留恢复前的加密副本（使用本次解密密码）
            </label>
            <Button
              variant="primary"
              isDisabled={!confirmed || busy}
              isPending={restore.isPending}
              onPress={() => restore.mutate()}
            >
              确认恢复
            </Button>
          </div>
        )}
        {error && <p className="text-sm text-danger">{toErrorMessage(error)}</p>}
      </Card.Content>
    </Card>
  );
}
