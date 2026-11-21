//! 连接监控与一键加规则业务服务。
//!
//! 从共享 `CoreRuntime` 读取活跃/已关闭连接与 SQLite 历史明细，支持经 Clash API
//! 断开单条或全部连接、清空历史统计，以及将命中的域名/IP/进程一键写入 `rules.toml`。

use std::net::IpAddr;
use std::sync::Arc;

use lanpath_client::{
    ConnRecordRow, ConnectionView, RecordsQuery, clash_close_all_connections,
    clash_close_connection,
};
use lanpath_common::{LanpathError, LanpathResult};

use crate::core_runtime::CoreRuntime;
use crate::rules::{Rules, rule_line};

/// 连接页一次拉取的快照。
#[derive(Debug, Clone, Default)]
pub(crate) struct ConnectionsSnapshot {
    pub(crate) active: Vec<ConnectionView>,
    pub(crate) closed: Vec<ConnectionView>,
    pub(crate) upload_total: u64,
    pub(crate) download_total: u64,
    pub(crate) clash_ready: bool,
}

/// 一键加规则的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuickRuleOutcome {
    pub(crate) line: String,
    pub(crate) already_exists: bool,
}

#[derive(Clone)]
pub(crate) struct ConnectionsService {
    runtime: Arc<CoreRuntime>,
    rules: Rules,
}

impl ConnectionsService {
    pub(crate) fn open() -> LanpathResult<Self> {
        let runtime = CoreRuntime::open()?;
        let rules = Rules::open()?;
        Ok(Self { runtime, rules })
    }

    /// 读取当前活跃连接与已关闭连接（内存环形缓冲优先，空时补充 SQLite 历史明细）。
    pub(crate) async fn snapshot(&self) -> ConnectionsSnapshot {
        let clash_ready = self.runtime.clash_api_endpoint().is_some();
        let (active, upload_total, download_total) = match self.runtime.active_connections().await {
            Some(act) => (act.connections, act.upload_total, act.download_total),
            None => (Vec::new(), 0, 0),
        };
        let mut closed = self.runtime.closed_connections().await;
        if closed.is_empty()
            && let Ok(store) = self.runtime.stats_store().await
            && let Ok(records) = store
                .query_records(&RecordsQuery {
                    limit: Some(200),
                    ..Default::default()
                })
                .await
        {
            closed = records.into_iter().map(record_to_view).collect();
        }
        ConnectionsSnapshot {
            active,
            closed,
            upload_total,
            download_total,
            clash_ready,
        }
    }

    /// 断开单条活跃连接。
    pub(crate) async fn close_connection(&self, id: &str) -> LanpathResult<()> {
        let (port, secret) = self
            .runtime
            .clash_api_endpoint()
            .ok_or_else(|| LanpathError::Client("核心未运行或 Clash API 未启用".into()))?;
        clash_close_connection(port, &secret, id).await
    }

    /// 断开全部活跃连接。
    pub(crate) async fn close_all_connections(&self) -> LanpathResult<()> {
        let (port, secret) = self
            .runtime
            .clash_api_endpoint()
            .ok_or_else(|| LanpathError::Client("核心未运行或 Clash API 未启用".into()))?;
        clash_close_all_connections(port, &secret).await
    }

    /// 清空本地 SQLite 流量统计与关闭连接记录。
    pub(crate) async fn clear_history(&self) -> LanpathResult<()> {
        let store = self.runtime.stats_store().await?;
        store.clear().await
    }

    /// 将指定连接一键写入 `rules.toml` 顶部（自动按域名/IP/进程生成规则并去重）。
    pub(crate) fn quick_add_rule(
        &self,
        conn: &ConnectionView,
        action: &str,
    ) -> LanpathResult<QuickRuleOutcome> {
        let line = build_rule_line(conn, action)?;
        let existing = self.rules.load()?;
        if existing.iter().any(|r| rule_line(r) == line) {
            return Ok(QuickRuleOutcome {
                line,
                already_exists: true,
            });
        }
        self.rules.add_lines(&line, true)?;
        Ok(QuickRuleOutcome {
            line,
            already_exists: false,
        })
    }
}

/// 生成供「自定义规则」跳转预填的规则行（默认动作 `proxy`）。
pub(crate) fn rule_seed_for(conn: &ConnectionView) -> String {
    build_rule_line(conn, "proxy").unwrap_or_default()
}

/// 根据连接的目标（域名 / IP / 进程）与目标动作生成一条标准规则文本。
pub(crate) fn build_rule_line(conn: &ConnectionView, action: &str) -> LanpathResult<String> {
    let action = action.trim();
    if action.is_empty() {
        return Err(LanpathError::Validation("规则动作不能为空".into()));
    }
    let target = conn.target.trim();
    if !target.is_empty() {
        if let Ok(ip) = target.parse::<IpAddr>() {
            let prefix = match ip {
                IpAddr::V4(_) => 32,
                IpAddr::V6(_) => 128,
            };
            return Ok(format!("cidr {ip}/{prefix} -> {action}"));
        }
        return Ok(format!("suffix {target} -> {action}"));
    }
    let process = conn.process.trim();
    if !process.is_empty() {
        return Ok(format!("process {process} -> {action}"));
    }
    Err(LanpathError::Validation(
        "该连接缺少域名、目的 IP 或进程信息，无法生成规则".into(),
    ))
}

fn record_to_view(row: ConnRecordRow) -> ConnectionView {
    ConnectionView {
        id: format!("rec-{}", row.id),
        host: row.host,
        target: row.target,
        destination_ip: row.destination_ip,
        outbound: row.outbound,
        network: row.network,
        process: String::new(),
        chain: row.chain,
        rule: row.rule,
        rule_payload: row.rule_payload,
        upload: row.upload.max(0) as u64,
        download: row.download.max(0) as u64,
        start: row.started_at.max(0) as u64,
    }
}

/// 将字节数格式化为紧凑人类可读单位（如 `1.4 MB`、`512 KB`、`0 B`）。
pub(crate) fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut idx = 0usize;
    while value >= 1024.0 && idx + 1 < UNITS.len() {
        value /= 1024.0;
        idx += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[idx])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[idx])
    } else {
        format!("{value:.2} {}", UNITS[idx])
    }
}

/// 将字节/秒格式化为速率单位（如 `2.3 MB/s`）。
pub(crate) fn format_rate(bps: u64) -> String {
    format!("{}/s", format_bytes(bps))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_conn(target: &str, process: &str) -> ConnectionView {
        ConnectionView {
            id: "c1".into(),
            host: target.into(),
            target: target.into(),
            destination_ip: String::new(),
            outbound: "proxy".into(),
            network: "tcp".into(),
            process: process.into(),
            chain: "proxy → HK-01".into(),
            rule: "MATCH".into(),
            rule_payload: String::new(),
            upload: 0,
            download: 0,
            start: 0,
        }
    }

    #[test]
    fn build_rule_line_handles_domain_ip_and_process() {
        assert_eq!(
            build_rule_line(&sample_conn("github.com", ""), "direct").unwrap(),
            "suffix github.com -> direct"
        );
        assert_eq!(
            build_rule_line(&sample_conn("1.1.1.1", ""), "proxy").unwrap(),
            "cidr 1.1.1.1/32 -> proxy"
        );
        assert_eq!(
            build_rule_line(&sample_conn("2606:4700::1", ""), "block").unwrap(),
            "cidr 2606:4700::1/128 -> block"
        );
        assert_eq!(
            build_rule_line(&sample_conn("", "curl"), "direct").unwrap(),
            "process curl -> direct"
        );
    }

    #[test]
    fn format_bytes_and_rate_readable() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(15 * 1024 * 1024), "15.0 MB");
        assert_eq!(format_rate(2048), "2.00 KB/s");
    }
}
