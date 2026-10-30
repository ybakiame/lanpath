//! Bounded, opt-in body inspection. Unmodified frames (including errors/trailers) replay verbatim.
use bytes::{Bytes, BytesMut};
use http::{
    HeaderMap, HeaderValue,
    header::{CONTENT_ENCODING, CONTENT_LENGTH, TRANSFER_ENCODING},
};
use http_body::{Body as HttpBody, Frame, SizeHint};
use http_body_util::BodyExt;
use hudsucker::Body;
use std::{
    collections::VecDeque,
    io::Read,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

type BodyFrame = Result<Frame<Bytes>, hudsucker::Error>;
pub(crate) fn headers_to_vec(headers: &HeaderMap) -> Vec<(String, String)> {
    // Header values are opaque octets, not necessarily UTF-8. Latin-1 is a reversible bridge.
    headers
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.as_bytes().iter().map(|b| char::from(*b)).collect(),
            )
        })
        .collect()
}
pub(crate) fn vec_to_headers(headers: &[(String, String)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        let bytes: Option<Vec<u8>> = value.chars().map(|c| u8::try_from(c as u32).ok()).collect();
        if let (Ok(name), Some(bytes)) =
            (http::header::HeaderName::from_bytes(name.as_bytes()), bytes)
            && let Ok(value) = HeaderValue::from_bytes(&bytes)
        {
            map.append(name, value);
        }
    }
    map
}

pub(crate) struct Inspected {
    frames: VecDeque<BodyFrame>,
    remainder: Body,
    pub bytes: Option<Vec<u8>>,
    pub exceeded: bool,
    trailers: Option<HeaderMap>,
    frame_bytes: u64,
    frame_meta: bool,
}

impl Inspected {
    pub fn exceeds_limit(&self, limit: usize) -> bool {
        self.exceeded
            || self.frame_bytes > limit as u64
            || self.bytes.as_ref().is_some_and(|bytes| bytes.len() > limit)
    }

    pub async fn read(mut body: Body, limit: usize, headers: &HeaderMap) -> Self {
        let mut frames = VecDeque::new();
        let mut bytes = BytesMut::new();
        let mut trailers = None;
        let mut complete = false;
        let mut frame_bytes = 0u64;
        let mut frame_meta = false;
        let mut exceeded = false;
        // Bound both payload and frame metadata; empty-frame streams cannot allocate forever.
        while frames.len() < 1024 {
            match body.frame().await {
                None => {
                    complete = true;
                    break;
                }
                Some(frame) => {
                    let stop = match &frame {
                        Ok(frame) => {
                            if let Some(data) = frame.data_ref() {
                                frame_bytes = frame_bytes.saturating_add(data.len() as u64);
                                if data.len() > limit.saturating_sub(bytes.len()) {
                                    exceeded = true;
                                    true
                                } else {
                                    bytes.extend_from_slice(data);
                                    false
                                }
                            } else {
                                if let Some(t) = frame.trailers_ref() {
                                    frame_meta = true;
                                    trailers = Some(t.clone());
                                }
                                false
                            }
                        }
                        Err(_) => {
                            frame_meta = true;
                            true
                        }
                    };
                    frames.push_back(frame);
                    if stop {
                        break;
                    }
                }
            }
        }
        let bytes = if complete {
            match decode(&bytes, headers, limit) {
                Ok(bytes) => Some(bytes),
                Err(DecodeFailure::TooLarge) => {
                    exceeded = true;
                    None
                }
                Err(DecodeFailure::Unavailable) => None,
            }
        } else {
            None
        };
        Self {
            frames,
            remainder: body,
            bytes,
            exceeded,
            trailers,
            frame_bytes,
            frame_meta,
        }
    }

    pub fn finish(self, replacement: Option<Vec<u8>>, headers: &mut HeaderMap) -> Body {
        if let Some(bytes) = replacement.filter(|b| self.bytes.as_ref() != Some(b)) {
            headers.remove(CONTENT_ENCODING);
            headers.remove(CONTENT_LENGTH);
            headers.remove(TRANSFER_ENCODING);
            let frame_bytes = bytes.len() as u64;
            let frame_meta = self.trailers.is_some();
            let mut frames = VecDeque::from([Ok(Frame::data(Bytes::from(bytes)))]);
            if let Some(trailers) = self.trailers {
                frames.push_back(Ok(Frame::trailers(trailers)));
            }
            Body::from(
                Replay {
                    frames,
                    remainder: Body::empty(),
                    frame_bytes,
                    frame_meta,
                }
                .boxed(),
            )
        } else {
            Body::from(
                Replay {
                    frames: self.frames,
                    remainder: self.remainder,
                    frame_bytes: self.frame_bytes,
                    frame_meta: self.frame_meta,
                }
                .boxed(),
            )
        }
    }
}

enum DecodeFailure {
    TooLarge,
    Unavailable,
}

fn decode(bytes: &[u8], headers: &HeaderMap, limit: usize) -> Result<Vec<u8>, DecodeFailure> {
    let mut encodings = Vec::new();
    for value in headers.get_all(CONTENT_ENCODING) {
        for encoding in value
            .to_str()
            .map_err(|_| DecodeFailure::Unavailable)?
            .split(',')
        {
            if encodings.len() == 8 {
                return Err(DecodeFailure::Unavailable);
            }
            encodings.push(encoding.trim().to_ascii_lowercase());
        }
    }
    let mut decoded = bytes.to_vec();
    for encoding in encodings.iter().rev() {
        if encoding == "identity" {
            continue;
        }
        let reader: Box<dyn Read + '_> = match encoding.as_str() {
            "gzip" | "x-gzip" => Box::new(flate2::read::MultiGzDecoder::new(decoded.as_slice())),
            "deflate" => Box::new(flate2::read::ZlibDecoder::new(decoded.as_slice())),
            "br" => Box::new(brotli::Decompressor::new(decoded.as_slice(), 4096)),
            _ => return Err(DecodeFailure::Unavailable),
        };
        let mut next = Vec::new();
        reader
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut next)
            .map_err(|_| DecodeFailure::Unavailable)?;
        if next.len() > limit {
            return Err(DecodeFailure::TooLarge);
        }
        decoded = next;
    }
    Ok(decoded)
}

#[derive(Debug)]
struct Replay {
    frames: VecDeque<BodyFrame>,
    remainder: Body,
    frame_bytes: u64,
    frame_meta: bool,
}
impl HttpBody for Replay {
    type Data = Bytes;
    type Error = hudsucker::Error;
    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<BodyFrame>> {
        if let Some(frame) = self.frames.pop_front() {
            if let Ok(frame) = &frame
                && let Some(data) = frame.data_ref()
            {
                self.frame_bytes -= data.len() as u64;
            }
            Poll::Ready(Some(frame))
        } else {
            Pin::new(&mut self.remainder).poll_frame(cx)
        }
    }
    fn is_end_stream(&self) -> bool {
        self.frames.is_empty() && self.remainder.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        let remaining = self.remainder.size_hint();
        let mut hint = SizeHint::new();
        hint.set_lower(self.frame_bytes.saturating_add(remaining.lower()));
        // Trailers/errors must still be polled, even when no data bytes remain.
        if !self.frame_meta
            && let Some(upper) = remaining
                .upper()
                .and_then(|value| value.checked_add(self.frame_bytes))
        {
            hint.set_upper(upper);
        }
        hint
    }
}

#[derive(Default)]
pub(crate) struct Capture {
    pub bytes: Vec<u8>,
    pub truncated: bool,
    pub complete: bool,
}
pub(crate) type SharedCapture = Arc<Mutex<Capture>>;
type Completion = Box<dyn FnOnce() + Send + Sync>;
struct Observed {
    body: Body,
    capture: SharedCapture,
    limit: usize,
    complete: Option<Completion>,
}
impl HttpBody for Observed {
    type Data = Bytes;
    type Error = hudsucker::Error;
    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<BodyFrame>> {
        let result = Pin::new(&mut self.body).poll_frame(cx);
        if let Poll::Ready(Some(Ok(frame))) = &result
            && let Some(bytes) = frame.data_ref()
        {
            let mut capture = self.capture.lock().unwrap_or_else(|p| p.into_inner());
            let len = self
                .limit
                .saturating_sub(capture.bytes.len())
                .min(bytes.len());
            capture.bytes.extend_from_slice(&bytes[..len]);
            capture.truncated |= len != bytes.len();
        }
        if matches!(result, Poll::Ready(None)) || self.body.is_end_stream() {
            self.capture
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .complete = true;
        }
        if matches!(result, Poll::Ready(Some(Err(_)))) {
            self.capture
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .truncated = true;
        }
        if (matches!(result, Poll::Ready(None) | Poll::Ready(Some(Err(_))))
            || self.body.is_end_stream())
            && let Some(complete) = self.complete.take()
        {
            complete();
        }
        result
    }
    fn size_hint(&self) -> SizeHint {
        self.body.size_hint()
    }
    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }
}
impl Drop for Observed {
    fn drop(&mut self) {
        if let Some(complete) = self.complete.take() {
            complete();
        }
    }
}
pub(crate) fn observe(
    body: Body,
    capture: SharedCapture,
    limit: usize,
    complete: Option<Completion>,
) -> Body {
    if body.is_end_stream() {
        capture.lock().unwrap_or_else(|p| p.into_inner()).complete = true;
    }
    Body::from(
        Observed {
            body,
            capture,
            limit,
            complete,
        }
        .boxed(),
    )
}
pub(crate) fn recorded(
    capture: &SharedCapture,
    headers: &HeaderMap,
    limit: usize,
) -> Option<String> {
    let capture = capture.lock().unwrap_or_else(|p| p.into_inner());
    // Binary, failed, oversized and incomplete streams are never recorded as successful text.
    if capture.truncated || !capture.complete {
        return None;
    }
    String::from_utf8(decode(&capture.bytes, headers, limit).ok()?).ok()
}
