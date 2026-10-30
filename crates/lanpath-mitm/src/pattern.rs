//! Pure-Rust regex compatibility with explicit resource budgets, not a wall-clock timeout.
use std::borrow::Cow;
use std::fmt;

pub const MAX_PATTERN_BYTES: usize = 16 * 1024;
pub const MAX_COMPILED_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_FALLBACK_INSTRUCTIONS: usize = 16 * 1024;
pub const MAX_FALLBACK_INPUT_BYTES: usize = 256 * 1024;
pub const MAX_BACKTRACKS: usize = 20_000;
pub const MAX_REPLACEMENT_BYTES: usize = 16 * 1024;
pub const MAX_FALLBACK_REPLACEMENTS: usize = 256;
/// Sum of full haystack lengths over iterator advances (including the final search).
pub const MAX_FALLBACK_SCAN_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_OUTPUT_GROWTH_BYTES: usize = 8 * 1024 * 1024;

/// Diagnostics deliberately contain neither the pattern nor traffic/capture contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternError {
    SourceLimit,
    CompileLimit,
    InvalidPattern,
    InputLimit,
    BacktrackLimit,
    Runtime,
    ReplacementLimit,
    OutputLimit,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SourceLimit => "regex source exceeds resource limit",
            Self::CompileLimit => "regex compilation exceeds resource limit",
            Self::InvalidPattern => "regex syntax or feature is unsupported",
            Self::InputLimit => "regex input exceeds resource limit",
            Self::BacktrackLimit => "regex backtracking exceeds resource limit",
            Self::Runtime => "regex execution failed",
            Self::ReplacementLimit => "regex replacement work exceeds resource limit",
            Self::OutputLimit => "regex replacement output exceeds resource limit",
        })
    }
}
impl std::error::Error for PatternError {}

#[derive(Clone)]
pub struct Pattern(Engine);

#[derive(Clone)]
enum Engine {
    Fast(regex::Regex),
    Fallback(fancy_regex::Regex),
}

impl fmt::Debug for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pattern").finish_non_exhaustive()
    }
}

impl Pattern {
    pub fn new(source: &str) -> Result<Self, PatternError> {
        if source.len() > MAX_PATTERN_BYTES {
            return Err(PatternError::SourceLimit);
        }
        match regex::RegexBuilder::new(source)
            .size_limit(MAX_COMPILED_BYTES)
            .dfa_size_limit(MAX_COMPILED_BYTES)
            .build()
        {
            Ok(regex) => return Ok(Self(Engine::Fast(regex))),
            Err(regex::Error::CompiledTooBig(_)) => return Err(PatternError::CompileLimit),
            Err(_) => {}
        }
        let regex = fancy_regex::RegexBuilder::new(source)
            .delegate_size_limit(MAX_COMPILED_BYTES)
            .delegate_dfa_size_limit(MAX_COMPILED_BYTES)
            .max_prog_size(MAX_FALLBACK_INSTRUCTIONS)
            .backtrack_limit(MAX_BACKTRACKS)
            .build()
            .map_err(|error| match error {
                fancy_regex::Error::CompileError(error)
                    if matches!(*error, fancy_regex::CompileError::PatternTooComplex) =>
                {
                    PatternError::CompileLimit
                }
                _ => PatternError::InvalidPattern,
            })?;
        Ok(Self(Engine::Fallback(regex)))
    }

    pub fn as_str(&self) -> &str {
        match &self.0 {
            Engine::Fast(regex) => regex.as_str(),
            Engine::Fallback(regex) => regex.as_str(),
        }
    }

    pub fn is_match(&self, input: &str) -> Result<bool, PatternError> {
        match &self.0 {
            Engine::Fast(regex) => Ok(regex.is_match(input)),
            Engine::Fallback(regex) => {
                fallback_input(input)?;
                regex.is_match(input).map_err(runtime_error)
            }
        }
    }

    pub fn replace<'a>(
        &self,
        input: &'a str,
        replacement: &str,
    ) -> Result<Cow<'a, str>, PatternError> {
        self.replacen(input, replacement, true)
    }

    pub fn replace_all<'a>(
        &self,
        input: &'a str,
        replacement: &str,
    ) -> Result<Cow<'a, str>, PatternError> {
        self.replacen(input, replacement, false)
    }

    fn replacen<'a>(
        &self,
        input: &'a str,
        replacement: &str,
        first_only: bool,
    ) -> Result<Cow<'a, str>, PatternError> {
        if replacement.len() > MAX_REPLACEMENT_BYTES {
            return Err(PatternError::ReplacementLimit);
        }
        if input.len() > MAX_OUTPUT_BYTES {
            return Err(PatternError::InputLimit);
        }
        match &self.0 {
            Engine::Fast(regex) if !replacement.contains('$') => replace_captures(
                regex.find_iter(input).map(|m| {
                    Ok(Capture::Literal {
                        start: m.start(),
                        end: m.end(),
                    })
                }),
                input,
                replacement,
                first_only,
                false,
            ),
            Engine::Fast(regex) => replace_captures(
                regex
                    .captures_iter(input)
                    .map(|caps| Ok(Capture::Fast(caps))),
                input,
                replacement,
                first_only,
                false,
            ),
            Engine::Fallback(regex) => {
                fallback_input(input)?;
                if !replacement.contains('$') {
                    return replace_captures(
                        regex.find_iter(input).map(|m| {
                            m.map(|m| Capture::Literal {
                                start: m.start(),
                                end: m.end(),
                            })
                            .map_err(runtime_error)
                        }),
                        input,
                        replacement,
                        first_only,
                        true,
                    );
                }
                replace_captures(
                    regex
                        .captures_iter(input)
                        .map(|caps| caps.map(Capture::Fallback).map_err(runtime_error)),
                    input,
                    replacement,
                    first_only,
                    true,
                )
            }
        }
    }
}

fn fallback_input(input: &str) -> Result<(), PatternError> {
    if input.len() > MAX_FALLBACK_INPUT_BYTES {
        Err(PatternError::InputLimit)
    } else {
        Ok(())
    }
}

fn runtime_error(error: fancy_regex::Error) -> PatternError {
    match error {
        fancy_regex::Error::RuntimeError(fancy_regex::RuntimeError::BacktrackLimitExceeded) => {
            PatternError::BacktrackLimit
        }
        _ => PatternError::Runtime,
    }
}

enum Capture<'a> {
    Literal { start: usize, end: usize },
    Fast(regex::Captures<'a>),
    Fallback(fancy_regex::Captures<'a, str>),
}
impl<'a> Capture<'a> {
    fn whole(&self) -> Result<(usize, usize), PatternError> {
        let range = match self {
            Self::Literal { start, end } => Some((*start, *end)),
            Self::Fast(caps) => caps.get(0).map(|m| (m.start(), m.end())),
            Self::Fallback(caps) => caps.get(0).map(|m| (m.start(), m.end())),
        };
        range.ok_or(PatternError::Runtime)
    }
    fn group(&self, name: &str) -> &'a str {
        let index = name.parse::<usize>().ok();
        match self {
            Self::Literal { .. } => None,
            Self::Fast(caps) => index
                .map_or_else(|| caps.name(name), |i| caps.get(i))
                .map(|m| m.as_str()),
            Self::Fallback(caps) => index
                .map_or_else(|| caps.name(name), |i| caps.get(i))
                .map(|m| m.as_str()),
        }
        .unwrap_or("")
    }
}

fn replace_captures<'a>(
    mut captures: impl Iterator<Item = Result<Capture<'a>, PatternError>>,
    input: &'a str,
    replacement: &str,
    first_only: bool,
    fallback: bool,
) -> Result<Cow<'a, str>, PatternError> {
    let limit = MAX_OUTPUT_BYTES.min(input.len().saturating_add(MAX_OUTPUT_GROWTH_BYTES));
    let max_matches = if fallback {
        MAX_FALLBACK_REPLACEMENTS
    } else {
        input.len().saturating_add(1)
    };
    let mut output: Option<String> = None;
    let mut copied_until = 0;
    let mut count = 0;
    let mut scan_bytes = 0usize;
    loop {
        if fallback {
            // fancy-regex can do at most two searches per advance when skipping an
            // empty match adjacent to a non-empty match. Together with the count cap
            // this bounds the entire operation to <= 2 * 257 * MAX_BACKTRACKS.
            scan_bytes = scan_bytes.saturating_add(input.len().max(1));
            if scan_bytes > MAX_FALLBACK_SCAN_BYTES {
                return Err(PatternError::ReplacementLimit);
            }
        }
        let Some(caps) = captures.next() else { break };
        let caps = caps?;
        if count == max_matches {
            return Err(PatternError::ReplacementLimit);
        }
        count += 1;
        let (start, end) = caps.whole()?;
        let matched = &input[start..end];
        let mut size = 0usize;
        let mut equal = true;
        expand(replacement, &caps, |part| {
            let next = size
                .checked_add(part.len())
                .ok_or(PatternError::OutputLimit)?;
            if next > limit {
                return Err(PatternError::OutputLimit);
            }
            equal &= matched.get(size..next) == Some(part);
            size = next;
            Ok(())
        })?;
        if !equal || size != matched.len() {
            let out = output.get_or_insert_with(|| String::with_capacity(input.len().min(limit)));
            append(out, &input[copied_until..start], limit)?;
            expand(replacement, &caps, |part| append(out, part, limit))?;
            copied_until = end;
        }
        if first_only {
            break;
        }
    }
    match output {
        None => Ok(Cow::Borrowed(input)),
        Some(mut output) => {
            append(&mut output, &input[copied_until..], limit)?;
            if output == input {
                Ok(Cow::Borrowed(input))
            } else {
                Ok(Cow::Owned(output))
            }
        }
    }
}

fn append(output: &mut String, part: &str, limit: usize) -> Result<(), PatternError> {
    if part.len() > limit.saturating_sub(output.len()) {
        return Err(PatternError::OutputLimit);
    }
    output.push_str(part);
    Ok(())
}

/// regex crate string interpolation, shared by both engines so fallback syntax cannot
/// change `$1suffix`, `${1}suffix`, `$$`, missing groups, or malformed references.
fn expand(
    mut replacement: &str,
    captures: &Capture<'_>,
    mut emit: impl FnMut(&str) -> Result<(), PatternError>,
) -> Result<(), PatternError> {
    while let Some(dollar) = replacement.find('$') {
        emit(&replacement[..dollar])?;
        let tail = &replacement[dollar + 1..];
        if let Some(rest) = tail.strip_prefix('$') {
            emit("$")?;
            replacement = rest;
            continue;
        }
        let reference = if let Some(braced) = tail.strip_prefix('{') {
            braced.find('}').map(|end| (&braced[..end], end + 2))
        } else {
            let end = tail
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                .count();
            (end != 0).then_some((&tail[..end], end))
        };
        if let Some((name, consumed)) = reference {
            emit(captures.group(name))?;
            replacement = &tail[consumed..];
        } else {
            emit("$")?;
            replacement = tail;
        }
    }
    emit(replacement)
}
