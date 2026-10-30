//! Lossless static-action serialization, independent from runtime regex compilation.
#[cfg(feature = "mitm")]
use lanpath_mitm::{Phase, RewriteKind, RewriteRule};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRewriteRule {
    pub pattern: String,
    pub kind: CachedRewriteKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CachedPhase {
    Request,
    Response,
}
#[cfg(feature = "mitm")]
impl From<Phase> for CachedPhase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Request => Self::Request,
            Phase::Response => Self::Response,
        }
    }
}
#[cfg(feature = "mitm")]
impl From<CachedPhase> for Phase {
    fn from(phase: CachedPhase) -> Self {
        match phase {
            CachedPhase::Request => Self::Request,
            CachedPhase::Response => Self::Response,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CachedRewriteKind {
    UrlRewrite {
        target: String,
    },
    Redirect {
        status: u16,
        target: String,
    },
    HeaderRewrite {
        phase: CachedPhase,
        name: String,
        value: Option<String>,
    },
    HeaderAppend {
        phase: CachedPhase,
        name: String,
        value: String,
    },
    HeaderReplace {
        phase: CachedPhase,
        name: String,
        value: String,
    },
    HeaderRemove {
        phase: CachedPhase,
        name: String,
    },
    HeaderValueRewrite {
        phase: CachedPhase,
        name: String,
        value_pattern: String,
        replacement: String,
    },
    HeaderBlockRewrite {
        phase: CachedPhase,
        block_pattern: String,
        replacement: String,
    },
    BodyRewrite {
        phase: CachedPhase,
        #[serde(default)]
        body_pattern: Option<String>,
        replacement: String,
    },
    BodyRewritePairs {
        phase: CachedPhase,
        pairs: Vec<(String, String)>,
    },
    StatusLineRewrite {
        pattern: String,
        replacement: String,
    },
    /// Old lossy text variant can be read for diagnostics, never emitted by new writers.
    Mock {
        status: u16,
        body: String,
        headers: Vec<(String, String)>,
    },
    MockBytes {
        status: u16,
        body: Vec<u8>,
        headers: Vec<(String, String)>,
    },
    Reject,
    RejectResponse {
        status: u16,
        body: Vec<u8>,
        content_type: Option<String>,
    },
}

#[cfg(feature = "mitm")]
impl From<&RewriteRule> for CachedRewriteRule {
    fn from(rule: &RewriteRule) -> Self {
        Self {
            pattern: rule.pattern.as_str().into(),
            kind: (&rule.kind).into(),
        }
    }
}
#[cfg(feature = "mitm")]
impl TryFrom<CachedRewriteRule> for RewriteRule {
    type Error = lanpath_mitm::PatternError;
    fn try_from(rule: CachedRewriteRule) -> Result<Self, Self::Error> {
        Ok(Self {
            pattern: lanpath_mitm::Pattern::new(&rule.pattern)?,
            kind: rule.kind.try_into()?,
        })
    }
}

#[cfg(feature = "mitm")]
impl From<&RewriteKind> for CachedRewriteKind {
    fn from(kind: &RewriteKind) -> Self {
        match kind {
            RewriteKind::UrlRewrite { target } => Self::UrlRewrite {
                target: target.clone(),
            },
            RewriteKind::Redirect { status, target } => Self::Redirect {
                status: *status,
                target: target.clone(),
            },
            RewriteKind::HeaderRewrite { phase, name, value } => Self::HeaderRewrite {
                phase: (*phase).into(),
                name: name.clone(),
                value: value.clone(),
            },
            RewriteKind::HeaderAppend { phase, name, value } => Self::HeaderAppend {
                phase: (*phase).into(),
                name: name.clone(),
                value: value.clone(),
            },
            RewriteKind::HeaderReplace { phase, name, value } => Self::HeaderReplace {
                phase: (*phase).into(),
                name: name.clone(),
                value: value.clone(),
            },
            RewriteKind::HeaderRemove { phase, name } => Self::HeaderRemove {
                phase: (*phase).into(),
                name: name.clone(),
            },
            RewriteKind::HeaderValueRewrite {
                phase,
                name,
                value_pattern,
                replacement,
            } => Self::HeaderValueRewrite {
                phase: (*phase).into(),
                name: name.clone(),
                value_pattern: value_pattern.as_str().into(),
                replacement: replacement.clone(),
            },
            RewriteKind::HeaderBlockRewrite {
                phase,
                block_pattern,
                replacement,
            } => Self::HeaderBlockRewrite {
                phase: (*phase).into(),
                block_pattern: block_pattern.as_str().into(),
                replacement: replacement.clone(),
            },
            RewriteKind::BodyRewrite {
                phase,
                body_pattern,
                replacement,
            } => Self::BodyRewrite {
                phase: (*phase).into(),
                body_pattern: body_pattern.as_ref().map(|p| p.as_str().into()),
                replacement: replacement.clone(),
            },
            RewriteKind::BodyRewritePairs { phase, pairs } => Self::BodyRewritePairs {
                phase: (*phase).into(),
                pairs: pairs
                    .iter()
                    .map(|(p, r)| (p.as_str().into(), r.clone()))
                    .collect(),
            },
            RewriteKind::StatusLineRewrite {
                pattern,
                replacement,
            } => Self::StatusLineRewrite {
                pattern: pattern.as_str().into(),
                replacement: replacement.clone(),
            },
            RewriteKind::Mock {
                status,
                body,
                headers,
            } => Self::MockBytes {
                status: *status,
                body: body.clone(),
                headers: headers.clone(),
            },
            RewriteKind::Reject {
                status,
                body,
                content_type,
            } => Self::RejectResponse {
                status: *status,
                body: body.clone(),
                content_type: content_type.clone(),
            },
        }
    }
}

#[cfg(feature = "mitm")]
impl TryFrom<CachedRewriteKind> for RewriteKind {
    type Error = lanpath_mitm::PatternError;
    fn try_from(kind: CachedRewriteKind) -> Result<Self, Self::Error> {
        Ok(match kind {
            CachedRewriteKind::UrlRewrite { target } => Self::UrlRewrite { target },
            CachedRewriteKind::Redirect { status, target } => Self::Redirect { status, target },
            CachedRewriteKind::HeaderRewrite { phase, name, value } => Self::HeaderRewrite {
                phase: phase.into(),
                name,
                value,
            },
            CachedRewriteKind::HeaderAppend { phase, name, value } => Self::HeaderAppend {
                phase: phase.into(),
                name,
                value,
            },
            CachedRewriteKind::HeaderReplace { phase, name, value } => Self::HeaderReplace {
                phase: phase.into(),
                name,
                value,
            },
            CachedRewriteKind::HeaderRemove { phase, name } => Self::HeaderRemove {
                phase: phase.into(),
                name,
            },
            CachedRewriteKind::HeaderValueRewrite {
                phase,
                name,
                value_pattern,
                replacement,
            } => Self::HeaderValueRewrite {
                phase: phase.into(),
                name,
                value_pattern: lanpath_mitm::Pattern::new(&value_pattern)?,
                replacement,
            },
            CachedRewriteKind::HeaderBlockRewrite {
                phase,
                block_pattern,
                replacement,
            } => Self::HeaderBlockRewrite {
                phase: phase.into(),
                block_pattern: lanpath_mitm::Pattern::new(&block_pattern)?,
                replacement,
            },
            CachedRewriteKind::BodyRewrite {
                phase,
                body_pattern,
                replacement,
            } => Self::BodyRewrite {
                phase: phase.into(),
                body_pattern: body_pattern
                    .map(|p| lanpath_mitm::Pattern::new(&p))
                    .transpose()?,
                replacement,
            },
            CachedRewriteKind::BodyRewritePairs { phase, pairs } => Self::BodyRewritePairs {
                phase: phase.into(),
                pairs: pairs
                    .into_iter()
                    .map(|(p, r)| lanpath_mitm::Pattern::new(&p).map(|p| (p, r)))
                    .collect::<Result<_, _>>()?,
            },
            CachedRewriteKind::StatusLineRewrite {
                pattern,
                replacement,
            } => Self::StatusLineRewrite {
                pattern: lanpath_mitm::Pattern::new(&pattern)?,
                replacement,
            },
            CachedRewriteKind::Mock {
                status,
                body,
                headers,
            } => Self::Mock {
                status,
                body: body.into_bytes(),
                headers,
            },
            CachedRewriteKind::MockBytes {
                status,
                body,
                headers,
            } => Self::Mock {
                status,
                body,
                headers,
            },
            CachedRewriteKind::Reject => Self::reject(),
            CachedRewriteKind::RejectResponse {
                status,
                body,
                content_type,
            } => Self::Reject {
                status,
                body,
                content_type,
            },
        })
    }
}
