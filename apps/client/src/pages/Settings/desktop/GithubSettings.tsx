import { Button, Card } from "@lanpath/ui";
import type { UseSettingsConfigReturn } from "./useSettingsConfig";
import { GithubPrefixField } from "../GithubAccessCard";
import { testGithubProxy, toErrorMessage } from "@lanpath/client-core";

interface GithubSettingsProps {
  settings: UseSettingsConfigReturn;
}

export default function GithubSettings({ settings }: GithubSettingsProps) {
  const {
    githubProxyPrefix,
    setGithubProxyPrefix,
    proxyTestPending,
    proxyTestResult,
    proxyTestError,
    setProxyTestPending,
    setProxyTestResult,
    setProxyTestError,
    persistDebounced,
  } = settings;

  const runProxyTest = async () => {
    setProxyTestPending(true);
    setProxyTestResult(null);
    setProxyTestError(null);
    try {
      const result = await testGithubProxy();
      const ms = result.match(/（(\d+) ms）/)?.[1];
      setProxyTestResult(ms ? `代理可用（${ms} ms）` : result);
    } catch (err) {
      setProxyTestError(toErrorMessage(err));
    }
    setProxyTestPending(false);
  };

  return (
    <Card>
      <Card.Header>
        <Card.Title>GitHub 访问</Card.Title>
        <Card.Description>仅改写 GitHub 资源地址；请求代理在「网络请求」中设置</Card.Description>
      </Card.Header>
      <Card.Content className="flex flex-col gap-4">
        <div className="flex flex-col gap-2">
          <GithubPrefixField
            settings={{
              ready: settings.config !== null,
              githubProxyPrefixDraft: githubProxyPrefix,
              onGithubProxyPrefixChange: (value) => {
                setGithubProxyPrefix(value);
                persistDebounced({ github_proxy_prefix: value });
              },
            }}
          />
          <div className="flex flex-wrap items-center gap-3">
            <Button variant="secondary" size="sm" isPending={proxyTestPending} onPress={() => void runProxyTest()}>
              测试连通性
            </Button>
            {proxyTestResult && <span className="break-words text-sm text-success">{proxyTestResult}</span>}
            {proxyTestError && <span className="break-words text-sm text-warning">{proxyTestError}</span>}
          </div>
        </div>
      </Card.Content>
    </Card>
  );
}
