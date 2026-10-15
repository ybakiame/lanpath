import { useMutation, useQuery } from "@tanstack/react-query";
import { Download } from "lucide-react";
import { BottomSheet, Button, Spinner } from "@lanpath/ui";
import {
  subscriptionSource,
  subscriptionSourceKey,
  exportSubscriptionSource,
  toastError,
  toastSuccess,
  toErrorMessage,
  type SubscriptionView,
} from "@lanpath/client-core";

export function SubscriptionSourceSheet({ sub, onClose }: { sub: SubscriptionView; onClose: () => void }) {
  const { data, isLoading, error } = useQuery({
    queryKey: subscriptionSourceKey(sub.id),
    queryFn: () => subscriptionSource(sub.id),
    retry: false,
    staleTime: 0,
    gcTime: 0,
  });
  const exportMutation = useMutation({
    mutationFn: () => exportSubscriptionSource(sub.id),
    onSuccess: (path) => toastSuccess(`原文已导出至 ${path}`),
    onError: (err) => toastError(toErrorMessage(err)),
  });
  return (
    <BottomSheet
      opened
      onClose={onClose}
      title={`订阅原文 · ${sub.name}`}
      footer={
        <Button
          variant="primary"
          className="min-h-11 w-full"
          isDisabled={!data}
          isPending={exportMutation.isPending}
          onPress={() => exportMutation.mutate()}
        >
          <Download className="size-4" aria-hidden="true" />
          导出{data ? ` .${data.extension}` : "原文"}
        </Button>
      }
    >
      <p className="text-xs text-muted">
        最近一次成功拉取的原始响应，保留原格式与注释。导出至下载目录的 Lanpath 文件夹。
      </p>
      {isLoading ? (
        <Spinner aria-label="读取订阅原文" />
      ) : error ? (
        <p role="alert" className="text-sm text-warning">
          {toErrorMessage(error)}
        </p>
      ) : (
        <pre className="mt-3 max-h-[55vh] overflow-auto whitespace-pre rounded-xl border border-border bg-surface-secondary p-3 font-mono text-xs text-foreground">
          {data?.content}
        </pre>
      )}
    </BottomSheet>
  );
}
