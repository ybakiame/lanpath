import { useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listSubscriptions, startProxy, stopProxy, toErrorMessage } from "../api";
import { START_PROXY_MUTATION_KEY, STOP_PROXY_MUTATION_KEY, SUBSCRIPTIONS_KEY } from "../api/keys";
import { writeRuntimeStatus } from "../runtimeQueries";
import type { ClientStatus, SubscriptionView } from "../api";

import { useCapabilities } from "./useCapabilities";
import { useClientConfig } from "./useClientConfig";
import { useProxyStatus } from "./useProxyStatus";

/** 双端共用运行数据与启停状态回写；授权重试由平台控制区负责。 */
export function useDashboardState({
  onActionError,
  onActionSuccess,
}: {
  onActionError: (message: string) => void;
  onActionSuccess: () => void;
}) {
  const queryClient = useQueryClient();
  const { data: config } = useClientConfig();
  const { data: status } = useProxyStatus();
  const { data: capabilities } = useCapabilities();
  const { data: subscriptions = [] } = useQuery<SubscriptionView[]>({
    queryKey: SUBSCRIPTIONS_KEY,
    queryFn: listSubscriptions,
    refetchInterval: 5000,
  });
  const writeStatus = async (next: ClientStatus) => {
    await writeRuntimeStatus(queryClient, next);
    onActionSuccess();
  };
  const reportError = (error: unknown) => onActionError(toErrorMessage(error));
  const startPending = useIsMutating({ mutationKey: START_PROXY_MUTATION_KEY }) > 0;
  const stopPending = useIsMutating({ mutationKey: STOP_PROXY_MUTATION_KEY }) > 0;
  const startMutation = useMutation({
    mutationKey: START_PROXY_MUTATION_KEY,
    mutationFn: startProxy,
    onSuccess: writeStatus,
    onError: reportError,
  });
  const stopMutation = useMutation({
    mutationKey: STOP_PROXY_MUTATION_KEY,
    mutationFn: stopProxy,
    onSuccess: writeStatus,
    onError: reportError,
  });
  const activeSub = subscriptions.find((sub) => sub.id === config?.active_subscription_id) ?? null;
  return {
    queryClient,
    config,
    status,
    capabilities,
    subscriptions,
    activeSub,
    running: status?.core_running ?? false,
    ruleMode: status?.rule_mode ?? config?.rule_mode ?? "rule",
    startMutation,
    stopMutation,
    startPending,
    stopPending,
  };
}
