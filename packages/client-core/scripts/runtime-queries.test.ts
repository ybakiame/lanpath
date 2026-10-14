import { expect, test } from "bun:test";
import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { writeRuntimeStatus } from "../src/runtimeQueries";
import { PROXIES_KEY, PROXY_STATUS_KEY } from "../src/api/keys";
import type { ClientStatus } from "../src/api/types";

const status: ClientStatus = {
  core_running: true,
  mitm_addr: null,
  system_proxy: false,
  rule_mode: "rule",
  rule_count: 0,
  clash_api_url: null,
  missing_rule_sets: [],
};

test("an already mounted proxy page immediately fetches the new core", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { staleTime: 30_000, retry: false } } });
  client.setQueryData(PROXIES_KEY, ["old subscription"]);
  let fetches = 0;
  const observer = new QueryObserver(client, {
    queryKey: PROXIES_KEY,
    queryFn: async () => {
      fetches++;
      return ["new subscription"];
    },
  });
  const unsubscribe = observer.subscribe(() => undefined);
  await writeRuntimeStatus(client, status);
  expect(fetches).toBe(1);
  expect(observer.getCurrentResult().data).toEqual(["new subscription"]);
  unsubscribe();
  client.clear();
});

test("stopping clears mounted runtime data without querying a stopped core", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { staleTime: 30_000, retry: false } } });
  client.setQueryData(PROXIES_KEY, ["old subscription"]);
  let fetches = 0;
  const observer = new QueryObserver(client, {
    queryKey: PROXIES_KEY,
    queryFn: async () => {
      fetches++;
      return ["old subscription"];
    },
  });
  const unsubscribe = observer.subscribe(() => undefined);
  await writeRuntimeStatus(client, { ...status, core_running: false });
  expect(fetches).toBe(0);
  expect(observer.getCurrentResult().data).toBeUndefined();
  unsubscribe();
  client.clear();
});

test("a new core discards fresh cached nodes before the next page query", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { staleTime: 30_000, retry: false } } });
  client.setQueryData(PROXIES_KEY, { nodes: ["old subscription"] });
  await writeRuntimeStatus(client, status);
  expect(client.getQueryData(PROXIES_KEY)).toBeUndefined();
  expect(client.getQueryData(PROXY_STATUS_KEY)).toEqual(status);
  const nodes = await client.fetchQuery({
    queryKey: PROXIES_KEY,
    queryFn: async () => ({ nodes: ["new subscription"] }),
  });
  expect(nodes.nodes).toEqual(["new subscription"]);
  client.clear();
});

test("late replies from the previous core cannot overwrite new nodes or start status", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  let oldNodes!: (value: string[]) => void;
  let oldStatus!: (value: ClientStatus) => void;
  const nodes = client
    .fetchQuery({
      queryKey: PROXIES_KEY,
      queryFn: () =>
        new Promise<string[]>((resolve) => {
          oldNodes = resolve;
        }),
    })
    .catch(() => undefined);
  const polling = client
    .fetchQuery({
      queryKey: PROXY_STATUS_KEY,
      queryFn: () =>
        new Promise<ClientStatus>((resolve) => {
          oldStatus = resolve;
        }),
    })
    .catch(() => undefined);
  await writeRuntimeStatus(client, status);
  await client.fetchQuery({ queryKey: PROXIES_KEY, queryFn: async () => ["new subscription"] });
  oldNodes(["old subscription"]);
  oldStatus({ ...status, core_running: false });
  await Promise.all([nodes, polling]);
  expect(client.getQueryData(PROXIES_KEY)).toEqual(["new subscription"]);
  expect(client.getQueryData(PROXY_STATUS_KEY)).toEqual(status);
  client.clear();
});
