//! Bounded wire-frame reading helpers for the MCP stdio transport (DEV-62).
//!
//! The primary transport is NDJSON (one JSON-RPC message per line). For
//! compatibility with hosts that speak LSP-style framing, a message whose
//! first line is a `Content-Length:` header is read as a framed body with the
//! same 10 MiB ceiling and chunked-read/error semantics the server has always
//! shipped. These helpers are extracted so chunk splits, UTF-8 splits, EOF
//! handling, and oversize drains are durably unit-tested.

use std::io::{BufRead, Read};

use super::{MAX_MCP_FRAME_BYTES, drain_framed_body};

#[derive(Debug)]
pub(super) enum FramedReadOutcome {
    /// Complete framed body decoded as UTF-8.
    Body(String),
    /// Framing problem; the detail string is suitable for a -32700 response.
    Malformed(String),
}

/// Outcome of one bounded NDJSON line read.
#[derive(Debug)]
pub(super) enum NdjsonLineOutcome {
    /// Complete line without its newline terminator (a trailing `\r` is
    /// stripped as well). A final line without a newline at EOF is still a
    /// line.
    Line(String),
    /// The line exceeded the maximum (same 10 MiB ceiling as framed bodies).
    /// The remainder of the line is drained so the stream stays in sync.
    Overlong,
    /// Stream ended with no pending bytes.
    Eof,
    /// The line is not valid UTF-8; the caller closes the stream.
    InvalidUtf8,
}

/// Read one NDJSON line with the same byte ceiling as framed bodies. The
/// accumulation is bounded: at most `MAX_MCP_FRAME_BYTES + 2` bytes are ever
/// buffered, and an overlong line's remainder is discarded in bounded chunks
/// until its newline so the next message can be parsed cleanly.
pub(super) fn read_ndjson_line<R: BufRead + ?Sized>(reader: &mut R) -> NdjsonLineOutcome {
    const DRAIN_CHUNK: u64 = 8192;
    let mut buffer: Vec<u8> = Vec::with_capacity(512);
    // Allow both CRLF terminator bytes in addition to the payload ceiling.
    let mut limited = (&mut *reader).take(MAX_MCP_FRAME_BYTES as u64 + 2);
    if limited.read_until(b'\n', &mut buffer).is_err() {
        return NdjsonLineOutcome::Eof;
    }

    if buffer.ends_with(b"\n") {
        buffer.pop();
        if buffer.ends_with(b"\r") {
            buffer.pop();
        }
        if buffer.len() > MAX_MCP_FRAME_BYTES {
            return NdjsonLineOutcome::Overlong;
        }
        match String::from_utf8(buffer) {
            Ok(line) => NdjsonLineOutcome::Line(line),
            Err(_) => NdjsonLineOutcome::InvalidUtf8,
        }
    } else if buffer.len() > MAX_MCP_FRAME_BYTES {
        // Overlong line: discard the rest of it (bounded chunks) so the
        // reader is positioned at the start of the next line.
        loop {
            let mut discard: Vec<u8> = Vec::with_capacity(DRAIN_CHUNK as usize);
            let mut chunk = (&mut *reader).take(DRAIN_CHUNK);
            let Ok(read) = chunk.read_until(b'\n', &mut discard) else {
                break;
            };
            if read == 0 || discard.ends_with(b"\n") {
                break;
            }
        }
        NdjsonLineOutcome::Overlong
    } else if buffer.is_empty() {
        NdjsonLineOutcome::Eof
    } else {
        // Final unterminated line at EOF.
        match String::from_utf8(buffer) {
            Ok(line) => NdjsonLineOutcome::Line(line),
            Err(_) => NdjsonLineOutcome::InvalidUtf8,
        }
    }
}
/// Read one framed message, given the already-consumed first header line
/// (which contained a case-insensitive `Content-Length:` prefix).
pub(super) fn read_framed_message<R: BufRead + ?Sized>(
    reader: &mut R,
    first_header_line: &str,
) -> FramedReadOutcome {
    let mut content_length: Option<usize> = None;
    if let Some(value) = header_value(first_header_line) {
        content_length = value.parse::<usize>().ok();
    }

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                return FramedReadOutcome::Malformed(format!("header read failed: {error}"));
            }
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = header_value(trimmed) {
            content_length = value.parse::<usize>().ok();
        }
    }

    let Some(length) = content_length else {
        return FramedReadOutcome::Malformed("missing Content-Length".to_string());
    };
    if length > MAX_MCP_FRAME_BYTES {
        drain_framed_body(reader, length);
        return FramedReadOutcome::Malformed(
            "Content-Length exceeds maximum frame size".to_string(),
        );
    }

    let mut buffer = vec![0u8; length];
    if let Err(error) = reader.read_exact(&mut buffer) {
        return FramedReadOutcome::Malformed(format!("body read failed: {error}"));
    }
    match String::from_utf8(buffer) {
        Ok(body) => FramedReadOutcome::Body(body),
        Err(error) => FramedReadOutcome::Malformed(format!("utf8 error: {error}")),
    }
}

fn header_value(line: &str) -> Option<&str> {
    let (name, value) = line.split_once(':')?;
    if name.trim().eq_ignore_ascii_case("content-length") {
        Some(value.trim())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    /// Reader that hands out at most `chunk` bytes per call, exercising the
    /// chunk-splitting paths (`read_exact` must loop internally).
    struct ChunkedReader {
        data: Vec<u8>,
        chunk: usize,
        position: usize,
    }

    impl ChunkedReader {
        fn new(data: Vec<u8>, chunk: usize) -> Self {
            Self {
                data,
                chunk,
                position: 0,
            }
        }
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let remaining = self.data.len() - self.position;
            if remaining == 0 || buf.is_empty() {
                return Ok(0);
            }
            let want = self.chunk.min(buf.len()).min(remaining);
            buf[..want].copy_from_slice(&self.data[self.position..self.position + want]);
            self.position += want;
            Ok(want)
        }
    }

    fn framed(body: &str) -> Vec<u8> {
        format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
    }

    #[test]
    fn reads_simple_frame_with_single_chunk() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let first = format!("Content-Length: {}", body.len());
        let mut cursor = Cursor::new(framed(body));
        match read_framed_message(&mut cursor, &first) {
            FramedReadOutcome::Body(decoded) => assert!(decoded.contains("ping")),
            other => panic!("expected body, got {other:?}"),
        }
    }

    #[test]
    fn reads_frame_with_exact_header_only() {
        let body = r#"{"jsonrpc":"2.0"}"#;
        let bytes = framed(body);
        // Pass the exact first header line including the parsed value.
        let first = format!("Content-Length: {}", body.len());
        let mut cursor = Cursor::new(bytes);
        match read_framed_message(&mut cursor, &first) {
            FramedReadOutcome::Body(decoded) => assert_eq!(decoded, body),
            other => panic!("expected body, got {other:?}"),
        }
    }

    #[test]
    fn reads_through_every_chunk_split_position_including_utf8_boundaries() {
        let body = format!("{{\"метод\":\"✅\",\"id\":{},\"нота\":\"текст\"}}", 12);
        let bytes = framed(&body);
        let first = format!("Content-Length: {}", body.len());
        for chunk in 1..=7 {
            let mut reader = std::io::BufReader::new(ChunkedReader::new(bytes.clone(), chunk));
            match read_framed_message(&mut reader, &first) {
                FramedReadOutcome::Body(decoded) => assert_eq!(
                    decoded, body,
                    "chunk size {chunk} must reassemble the full UTF-8 body"
                ),
                other => panic!("chunk {chunk}: expected body, got {other:?}"),
            }
        }
    }

    #[test]
    fn later_content_length_header_wins() {
        let body = r#"{"a":1}"#;
        let bytes = format!(
            "Content-Length: 999\r\nSome-Header: x\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .into_bytes();
        let mut cursor = Cursor::new(bytes);
        match read_framed_message(&mut cursor, "Content-Length: 999") {
            FramedReadOutcome::Body(decoded) => assert_eq!(decoded, body),
            other => panic!("expected body, got {other:?}"),
        }
    }

    #[test]
    fn invalid_content_length_is_reported_as_missing() {
        let bytes = b"Content-Length: abc\r\n\r\n{}".to_vec();
        let mut cursor = Cursor::new(bytes);
        match read_framed_message(&mut cursor, "Content-Length: abc") {
            FramedReadOutcome::Malformed(details) => {
                assert_eq!(details, "missing Content-Length")
            }
            other => panic!("expected malformed, got {other:?}"),
        }
    }

    #[test]
    fn header_section_without_content_length_is_reported_missing() {
        let bytes = b"X-Custom: 1\r\n\r\n{}".to_vec();
        let mut cursor = Cursor::new(bytes);
        // The caller only enters the framed path when the first line carries a
        // Content-Length header; simulate one that fails to parse.
        match read_framed_message(&mut cursor, "Content-Length: not-a-number") {
            FramedReadOutcome::Malformed(details) => {
                assert_eq!(details, "missing Content-Length")
            }
            other => panic!("expected malformed, got {other:?}"),
        }
    }

    #[test]
    fn eof_mid_body_reports_body_read_failure() {
        let body = r#"{"jsonrpc":"2.0","id":1}"#;
        let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        bytes.extend_from_slice(body.as_bytes());
        bytes.truncate(bytes.len() - 5); // truncate inside the body
        let first = format!("Content-Length: {}", body.len());
        let mut cursor = Cursor::new(bytes);
        match read_framed_message(&mut cursor, &first) {
            FramedReadOutcome::Malformed(details) => {
                assert!(details.starts_with("body read failed"), "got: {details}")
            }
            other => panic!("expected malformed, got {other:?}"),
        }
    }

    #[test]
    fn invalid_utf8_body_reports_utf8_error() {
        let mut bytes = b"Content-Length: 4\r\n\r\n".to_vec();
        bytes.extend_from_slice(&[0xff, 0xfe, 0xfd, 0xfc]);
        let mut cursor = Cursor::new(bytes);
        match read_framed_message(&mut cursor, "Content-Length: 4") {
            FramedReadOutcome::Malformed(details) => {
                assert!(details.starts_with("utf8 error"), "got: {details}")
            }
            other => panic!("expected malformed, got {other:?}"),
        }
    }

    #[test]
    fn oversize_frame_is_drained_and_the_stream_stays_in_sync() {
        let junk_len = MAX_MCP_FRAME_BYTES + 1;
        let mut bytes = format!("Content-Length: {junk_len}\r\n\r\n").into_bytes();
        bytes.extend(std::iter::repeat_n(b'x', junk_len));
        // A follow-up valid NDJSON message the server must still be able to
        // read after the drain (the wire loop keeps reading lines).
        let followup = format!("{}\n", r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#);
        bytes.extend_from_slice(followup.as_bytes());

        let mut buffered = std::io::BufReader::new(ChunkedReader::new(bytes, 1024));
        match read_framed_message(&mut buffered, &format!("Content-Length: {junk_len}")) {
            FramedReadOutcome::Malformed(details) => {
                assert_eq!(details, "Content-Length exceeds maximum frame size")
            }
            other => panic!("expected malformed, got {other:?}"),
        }
        // After the drain, the next line must be intact.
        let mut line = String::new();
        buffered.read_line(&mut line).unwrap();
        assert!(
            line.contains("\"ping\""),
            "stream must stay in sync, got: {line}"
        );
    }

    #[test]
    fn ndjson_reads_simple_empty_and_partial_eof_lines() {
        use super::NdjsonLineOutcome;

        let bytes = b"first\n\nlast".to_vec();
        let mut cursor = Cursor::new(bytes);
        match read_ndjson_line(&mut cursor) {
            NdjsonLineOutcome::Line(line) => assert_eq!(line, "first"),
            other => panic!("expected line, got {other:?}"),
        }
        match read_ndjson_line(&mut cursor) {
            NdjsonLineOutcome::Line(line) => assert_eq!(line, ""),
            other => panic!("expected empty line, got {other:?}"),
        }
        // Unterminated final line at EOF is still delivered.
        match read_ndjson_line(&mut cursor) {
            NdjsonLineOutcome::Line(line) => assert_eq!(line, "last"),
            other => panic!("expected partial line, got {other:?}"),
        }
        match read_ndjson_line(&mut cursor) {
            NdjsonLineOutcome::Eof => {}
            other => panic!("expected eof, got {other:?}"),
        }
    }

    #[test]
    fn ndjson_reassembles_chunked_utf8_and_strips_crlf() {
        let payload = "{\"метод\":\"✅\",\"текст\":\"строка\"}";
        let mut bytes = format!("{payload}\r\n").into_bytes();
        bytes.extend_from_slice(b"{\"second\":1}\n");
        // One byte per read forces chunk-split reassembly across the
        // multi-byte characters.
        let mut reader = std::io::BufReader::new(ChunkedReader::new(bytes, 1));
        match read_ndjson_line(&mut reader) {
            NdjsonLineOutcome::Line(line) => assert_eq!(line, payload),
            other => panic!("expected line, got {other:?}"),
        }
        let mut buffered = std::io::BufReader::new(reader);
        match read_ndjson_line(&mut buffered) {
            NdjsonLineOutcome::Line(line) => assert_eq!(line, "{\"second\":1}"),
            other => panic!("expected line, got {other:?}"),
        }
    }

    #[test]
    fn ndjson_invalid_utf8_is_reported() {
        let mut bytes = b"\xff\xfe\n".to_vec();
        bytes.extend_from_slice(b"{}\n");
        let mut cursor = Cursor::new(bytes);
        match read_ndjson_line(&mut cursor) {
            NdjsonLineOutcome::InvalidUtf8 => {}
            other => panic!("expected invalid utf8, got {other:?}"),
        }
    }

    #[test]
    fn ndjson_line_at_exact_limit_is_valid_but_one_more_byte_is_overlong() {
        let at_limit = "a".repeat(MAX_MCP_FRAME_BYTES);
        let mut bytes = at_limit.clone().into_bytes();
        bytes.push(b'\n');
        bytes.extend_from_slice(b"{}\n");
        let mut cursor = Cursor::new(bytes);
        match read_ndjson_line(&mut cursor) {
            NdjsonLineOutcome::Line(line) => assert_eq!(line.len(), MAX_MCP_FRAME_BYTES),
            other => panic!("expected line, got {other:?}"),
        }

        let mut crlf_bytes = at_limit.into_bytes();
        crlf_bytes.extend_from_slice(b"\r\n{}\n");
        let mut crlf_cursor = Cursor::new(crlf_bytes);
        match read_ndjson_line(&mut crlf_cursor) {
            NdjsonLineOutcome::Line(line) => assert_eq!(line.len(), MAX_MCP_FRAME_BYTES),
            other => panic!("expected CRLF line at limit, got {other:?}"),
        }
        match read_ndjson_line(&mut crlf_cursor) {
            NdjsonLineOutcome::Line(line) => assert_eq!(line, "{}"),
            other => panic!("expected next line after CRLF, got {other:?}"),
        }

        let over_limit = "a".repeat(MAX_MCP_FRAME_BYTES + 1);
        let mut bytes = over_limit.into_bytes();
        bytes.push(b'\n');
        // Follow-up message must still parse after the overlong drain.
        bytes.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n");
        let mut buffered = std::io::BufReader::new(ChunkedReader::new(bytes, 64 * 1024));
        match read_ndjson_line(&mut buffered) {
            NdjsonLineOutcome::Overlong => {}
            other => panic!("expected overlong, got {other:?}"),
        }
        match read_ndjson_line(&mut buffered) {
            NdjsonLineOutcome::Line(line) => {
                assert!(line.contains("\"ping\""), "stream resyncs, got: {line}")
            }
            other => panic!("expected line, got {other:?}"),
        }
    }

    #[test]
    fn ndjson_overlong_without_trailing_newline_drains_to_eof() {
        // Hostile peer sends an oversized line then closes mid-line: the
        // bounded reader must terminate instead of looping or allocating.
        let over_limit = "a".repeat(MAX_MCP_FRAME_BYTES + 5);
        let mut reader =
            std::io::BufReader::new(ChunkedReader::new(over_limit.into_bytes(), 64 * 1024));
        match read_ndjson_line(&mut reader) {
            NdjsonLineOutcome::Overlong => {}
            other => panic!("expected overlong, got {other:?}"),
        }
        match read_ndjson_line(&mut reader) {
            NdjsonLineOutcome::Eof => {}
            other => panic!("expected eof, got {other:?}"),
        }
    }
}
