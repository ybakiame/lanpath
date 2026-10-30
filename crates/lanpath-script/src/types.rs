use serde::{Deserialize, Serialize};

/// 脚本方言（模拟的客户端代理软件 API 风格）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptDialect {
    /// Quantumult X 风格（$task / $prefs / $notify）。
    QuantumultX,
    /// Surge 风格（$httpClient / $persistentStore / $notification）。
    Surge,
    /// Loon public script APIs (not a Quantumult X superset).
    Loon,
    /// Stash public QuickJS script host APIs.
    Stash,
}

/// 脚本类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptKind {
    /// http-request：arg 为请求描述，$done 返回修改后的请求。
    HttpRequest,
    /// http-response：arg 为响应描述，$done 返回修改后的响应。
    HttpResponse,
    /// cron 定时脚本。
    Cron,
    /// 通用脚本。
    Generic,
}

/// Source identity and rule policy retained for one real execution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptExecutionMetadata {
    pub name: String,
    pub source_id: String,
    pub module_id: Option<String>,
    pub binary_body_mode: bool,
}

/// Transport protocol preference; automatic negotiation preserves non-Loon hosts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpAlpn {
    #[default]
    Auto,
    Http1,
    Http2,
}

/// 一次 HTTP 请求的描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpRequestSpec {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout_ms: Option<u64>,
    /// Read and update this run's private cookie jar. False bypasses it entirely.
    pub auto_cookie: bool,
    pub auto_redirect: bool,
    pub insecure: bool,
    #[serde(default)]
    pub alpn: HttpAlpn,
}

impl Default for HttpRequestSpec {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: String::new(),
            headers: Vec::new(),
            body: None,
            timeout_ms: None,
            auto_cookie: false,
            auto_redirect: true,
            insecure: false,
            alpn: HttpAlpn::Auto,
        }
    }
}

/// 一次 HTTP 响应的数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpResponseData {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Undefined completion is not an explicit null (which aborts in Loon/Stash).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScriptCompletion {
    Undefined,
    Value,
}

/// Strings remain strings; binary values are represented as byte arrays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptOutput(pub serde_json::Value, pub ScriptCompletion);

impl Default for ScriptOutput {
    fn default() -> Self {
        Self(serde_json::Value::Null, ScriptCompletion::Undefined)
    }
}

/// 脚本执行资源限制。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptLimits {
    /// 整体执行超时（毫秒），默认 5000。
    pub timeout_ms: u64,
    /// QuickJS 运行时内存上限（字节），默认 32MB。
    pub memory_limit_bytes: usize,
}

impl Default for ScriptLimits {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            memory_limit_bytes: 32 * 1024 * 1024,
        }
    }
}
