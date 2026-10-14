import type { QueryClient } from "@tanstack/react-query";
import type { ClientStatus } from "./api/types";
import { PROXY_STATUS_KEY, PROXIES_KEY, CONNECTIONS_KEY } from "./api/keys";

/** 启停响应替代旧状态；取消上一核心的请求并重置运行数据，防止旧节点跨启动复用。 */
export async function writeRuntimeStatus(queryClient: QueryClient, status: ClientStatus): Promise<void> {
  await queryClient.cancelQueries({ queryKey: PROXY_STATUS_KEY });
  await Promise.all([PROXIES_KEY, CONNECTIONS_KEY].map((queryKey) => queryClient.cancelQueries({ queryKey })));
  queryClient.setQueryData(PROXY_STATUS_KEY, status);
  for (const queryKey of [PROXIES_KEY, CONNECTIONS_KEY]) {
    // resetQueries 会自动重拉已挂载查询；停止时只重置，避免向已停止的核心发请求。
    for (const query of queryClient.getQueryCache().findAll({ queryKey })) query.reset();
  }
  if (status.core_running) {
    await Promise.all(
      [PROXIES_KEY, CONNECTIONS_KEY].map((queryKey) => queryClient.refetchQueries({ queryKey, type: "active" })),
    );
  }
}
