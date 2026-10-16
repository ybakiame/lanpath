import { useState } from "react";
import { Card, Button, SelectField } from "@lanpath/ui";
import { useClientConfig, useSettingsPersist } from "@lanpath/client-core";
import type { ClientConfig } from "@lanpath/client-core";
import { SubPageShell } from "../../components/PageShell";
import { SettingsInput } from "./fields";

const options = [
  { value: "none", label: "不使用代理", description: "忽略系统与环境代理，直接连接" },
  { value: "system", label: "系统代理", description: "跟随系统或环境代理，未配置时直连" },
  { value: "core", label: "核心代理", description: "通过 Lanpath 本地核心连接，需要核心运行中" },
  { value: "custom", label: "自定义代理", description: "使用指定的 HTTP(S) 或 SOCKS5 代理" },
];

function addressError(value: string): string | null {
  try {
    const url = new URL(value.trim());
    if (
      !["http:", "https:", "socks5:", "socks5h:"].includes(url.protocol) ||
      !url.hostname ||
      url.port === "0" ||
      !["", "/"].includes(url.pathname) ||
      url.search ||
      url.hash
    ) {
      return "请填写有效的 HTTP(S) 或 SOCKS5 代理地址";
    }
    return null;
  } catch {
    return "请填写完整代理地址，例如 http://127.0.0.1:7890";
  }
}

function RequestSettings({ config }: { config: ClientConfig }) {
  const engine = useSettingsPersist();
  const [mode, setMode] = useState(config.request_proxy_mode);
  const [address, setAddress] = useState(config.request_proxy_url);
  const error = mode === "custom" ? addressError(address) : null;
  const changed = mode !== config.request_proxy_mode || address.trim() !== config.request_proxy_url;

  return (
    <Card>
      <Card.Header>
        <Card.Title>请求代理</Card.Title>
        <Card.Description>用于下载订阅、核心和远程资源，独立于 GitHub 镜像前缀。</Card.Description>
      </Card.Header>
      <Card.Content className="flex flex-col gap-4">
        <SelectField
          label="代理方式"
          value={mode}
          options={options}
          disabled={engine.saving}
          onChange={(value) => setMode(value as ClientConfig["request_proxy_mode"])}
        />
        <p className="text-sm text-muted">{options.find((option) => option.value === mode)?.description}</p>
        {mode === "system" && (
          <p className="text-xs text-muted">Linux 主要读取代理环境变量。已配置的代理不可用时，请求会报错。</p>
        )}
        {mode === "custom" && (
          <SettingsInput
            id="request-proxy-url"
            label="代理地址"
            value={address}
            onChange={setAddress}
            placeholder="http://127.0.0.1:7890"
            inputMode="url"
            disabled={engine.saving}
            error={address.trim() ? error : null}
            hint="支持 http://、https://、socks5://、socks5h://，可包含用户名与密码"
          />
        )}
        {mode === "core" && <p className="text-xs text-muted">本地混合端口：127.0.0.1:{config.mixed_port}</p>}
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="primary"
            isPending={engine.saving}
            isDisabled={!changed || Boolean(error)}
            onPress={() => void engine.persist({ request_proxy_mode: mode, request_proxy_url: address.trim() })}
          >
            保存
          </Button>
          <span className="text-xs text-muted">保存后对下一次请求生效，无需重启核心</span>
        </div>
      </Card.Content>
    </Card>
  );
}

export default function NetworkRequestsPage() {
  const { data: config, error } = useClientConfig();
  return (
    <SubPageShell title="网络请求">
      {config ? (
        <RequestSettings key={`${config.request_proxy_mode}:${config.request_proxy_url}`} config={config} />
      ) : (
        <output className="text-sm text-muted">{error ? "无法加载网络请求设置，请稍后重试" : "正在加载设置…"}</output>
      )}
    </SubPageShell>
  );
}
