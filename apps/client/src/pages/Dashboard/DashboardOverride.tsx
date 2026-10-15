import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { Card, SelectField } from "@lanpath/ui";
import {
  SUBSCRIPTION_PROFILE_MUTATION_KEY,
  PROFILES_KEY,
  SUBSCRIPTIONS_KEY,
  CONFIG_PREVIEW_KEY,
  listProfiles,
  setSubscriptionProfile,
  markRestartRequired,
  toastSuccess,
  toastError,
  toErrorMessage,
  type SubscriptionView,
} from "@lanpath/client-core";

/** Overrides remain associated with the selected subscription, shared on both targets. */
export function DashboardOverride({
  sub,
  running,
  disabled = false,
}: {
  sub: SubscriptionView | null;
  running: boolean;
  disabled?: boolean;
}) {
  const queryClient = useQueryClient();
  const { data: profiles = [], isLoading, error } = useQuery({ queryKey: PROFILES_KEY, queryFn: listProfiles });
  const mutation = useMutation({
    mutationKey: SUBSCRIPTION_PROFILE_MUTATION_KEY,
    mutationFn: async (profileId: string) => {
      if (!sub) throw new Error("请先选择订阅");
      await setSubscriptionProfile(sub.id, profileId || null);
      return { id: sub.id, profileId: profileId || null };
    },
    onSuccess: async ({ id, profileId }) => {
      queryClient.setQueryData<SubscriptionView[]>(SUBSCRIPTIONS_KEY, (subs) =>
        subs?.map((item) => (item.id === id ? { ...item, profile_id: profileId } : item)),
      );
      markRestartRequired("subscription", running);
      await queryClient.invalidateQueries({ queryKey: CONFIG_PREVIEW_KEY });
      toastSuccess(running ? "覆写已保存，重启代理后生效" : "覆写已保存，下次启动生效");
    },
    onError: (err) => toastError(toErrorMessage(err)),
  });
  const missing = sub?.profile_id && !profiles.some((p) => p.id === sub.profile_id);
  return (
    <Card>
      <Card.Header>
        <Card.Title>订阅覆写</Card.Title>
        <Card.Description>{sub ? `配置「${sub.name}」使用的模板` : "选择生效订阅后配置覆写"}</Card.Description>
      </Card.Header>
      <Card.Content className="flex flex-col gap-2">
        <SelectField
          label="覆写模板"
          value={sub?.profile_id ?? "__none"}
          disabled={!sub || disabled || isLoading || !!error || mutation.isPending}
          onChange={(value) => mutation.mutate(value === "__none" ? "" : value)}
          options={[
            { value: "__none", label: "不使用覆写" },
            ...profiles.map((p) => ({ value: p.id, label: p.name })),
            ...(missing ? [{ value: sub?.profile_id ?? "", label: "原模板已失效，请重新选择" }] : []),
          ]}
        />
        {error && (
          <p role="alert" className="text-xs text-warning">
            加载覆写模板失败：{toErrorMessage(error)}
          </p>
        )}
        <p className="text-xs text-muted">仅影响当前订阅的运行配置，订阅原文保持不变。模板内容可单独管理。</p>
        <Link to="/override" className="inline-flex min-h-11 items-center text-sm text-accent">
          管理覆写模板
        </Link>
      </Card.Content>
    </Card>
  );
}
