import { useState } from "react";
import type { SubscriptionView } from "@lanpath/client-core";
import { BottomSheet, Button, inputClassName, IS_MOBILE } from "@lanpath/ui";

export interface SubscriptionDraft {
  name: string;
  url: string;
  /** 拉取请求的 User-Agent（空 = 默认 `clash.meta`）。 */
  userAgent: string;
}

interface SubscriptionFormProps {
  isOpen: boolean;
  /** `null` = 添加模式；非空 = 编辑该订阅（表单预填）。 */
  editing: SubscriptionView | null;
  /** 保存进行中（提交按钮 loading 且禁用表单）。 */
  busy: boolean;
  onClose: () => void;
  onSave: (draft: SubscriptionDraft) => void;
}

/** 表单字段标签行样式。 */
const labelClassName = "text-sm font-medium text-foreground";

/** 共用订阅表单：桌面 Dialog / 移动 Sheet；编辑失败保留草稿。 */
export function SubscriptionForm({ isOpen, editing, busy, onClose, onSave }: SubscriptionFormProps) {
  const [draft, setDraft] = useState<SubscriptionDraft>({
    name: "",
    url: "",
    userAgent: "",
  });
  const [prevKey, setPrevKey] = useState<string | null>(null);
  // open 切换（添加空表单 / 编辑预填）时同步表单初始值。
  const key = isOpen ? (editing?.id ?? "__add__") : null;
  if (key !== prevKey) {
    setPrevKey(key);
    setDraft(
      editing
        ? {
            name: editing.name,
            url: editing.url,
            userAgent: editing.user_agent ?? "",
          }
        : { name: "", url: "", userAgent: "" },
    );
  }

  const formValid = draft.name.trim().length > 0 && draft.url.trim().length > 0;

  const handleSave = () => {
    if (!formValid || busy) return;
    onSave({
      name: draft.name.trim(),
      url: draft.url.trim(),
      userAgent: draft.userAgent.trim(),
    });
  };

  return (
    <BottomSheet
      opened={isOpen}
      onClose={onClose}
      title={editing ? "编辑订阅" : "添加订阅"}
      footer={
        <Button
          variant="primary"
          className="min-h-12 w-full"
          isDisabled={!formValid}
          isPending={busy}
          onPress={handleSave}
        >
          {editing ? "保存" : "添加"}
        </Button>
      }
    >
      <div className="flex flex-col gap-4">
        <label className="flex flex-col gap-1.5">
          <span className={labelClassName}>名称</span>
          <input
            value={draft.name}
            onChange={(event) => setDraft((prev) => ({ ...prev, name: event.target.value }))}
            placeholder="我的机场"
            aria-required="true"
            disabled={busy}
            className={inputClassName}
          />
        </label>
        <label className="flex flex-col gap-1.5">
          <span className={labelClassName}>URL</span>
          <input
            type="url"
            value={draft.url}
            onChange={(event) => setDraft((prev) => ({ ...prev, url: event.target.value }))}
            placeholder="https://example.com/sub"
            aria-required="true"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            inputMode="url"
            disabled={busy}
            className={`${inputClassName} font-mono`}
          />
        </label>
        <label className="flex flex-col gap-1.5">
          <span className={labelClassName}>User-Agent（可选）</span>
          <input
            value={draft.userAgent}
            onChange={(event) => setDraft((prev) => ({ ...prev, userAgent: event.target.value }))}
            placeholder="留空使用默认 clash.meta"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            disabled={busy}
            className={`${inputClassName} font-mono`}
          />
          <span className="text-xs text-muted">部分订阅源按 UA 返回不同格式（如 clash / sing-box）</span>
        </label>
        {!IS_MOBILE && (
          <div className="flex flex-wrap gap-2">
            {["", "clash.meta", "clash-verge", "sing-box"].map((value) => (
              <Button
                key={value}
                size="sm"
                variant="secondary"
                isDisabled={busy}
                onPress={() => setDraft((prev) => ({ ...prev, userAgent: value }))}
              >
                {value || "默认 UA"}
              </Button>
            ))}
          </div>
        )}
      </div>
    </BottomSheet>
  );
}
