use courier_proto::AttachmentId;
use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("message is missing a MIME body")]
    MissingBody,
    #[error("invalid multipart boundary: {0}")]
    InvalidBoundary(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyKind {
    PlainText,
    Html,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedBody {
    pub kind: BodyKind,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedAttachment {
    pub id: AttachmentId,
    pub filename: String,
    pub mime_type: String,
    pub content_id: Option<String>,
    pub inline: bool,
    pub size: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedMessage {
    pub headers: ParsedHeaders,
    pub body: ParsedBody,
    pub attachments: Vec<ParsedAttachment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedHeaders {
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub message_id: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MimePart {
    headers: Headers,
    body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Headers(Vec<(String, String)>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContentType {
    media_type: String,
    boundary: Option<String>,
    name: Option<String>,
    charset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContentDisposition {
    disposition: String,
    filename: Option<String>,
}

#[derive(Debug, Default)]
struct MessageCollector {
    plain: Option<String>,
    html: Option<String>,
    attachments: Vec<ParsedAttachment>,
}

pub fn parse_rfc822(raw: &[u8]) -> Result<ParsedMessage> {
    let raw = String::from_utf8_lossy(raw);
    let root = parse_part(&raw)?;
    let mut collector = MessageCollector::default();
    collect_part(&root, &mut collector)?;

    Ok(ParsedMessage {
        headers: parsed_headers(&root.headers),
        body: choose_display_body(collector.plain, collector.html),
        attachments: collector.attachments,
    })
}

pub fn choose_display_body(plain: Option<String>, html: Option<String>) -> ParsedBody {
    if let Some(html) = html {
        ParsedBody {
            kind: BodyKind::Html,
            content: html,
        }
    } else {
        ParsedBody {
            kind: BodyKind::PlainText,
            content: plain.unwrap_or_default(),
        }
    }
}

fn collect_part(part: &MimePart, collector: &mut MessageCollector) -> Result<()> {
    let content_type = part.content_type();

    if content_type.media_type.starts_with("multipart/") {
        let boundary = content_type
            .boundary
            .ok_or_else(|| Error::InvalidBoundary(content_type.media_type.clone()))?;

        for child in split_multipart(&part.body, &boundary)? {
            collect_part(&child, collector)?;
        }
        return Ok(());
    }

    let disposition = part.content_disposition();
    let decoded = decode_transfer(part.body.as_bytes(), part.transfer_encoding());
    let filename = disposition
        .as_ref()
        .and_then(|disposition| disposition.filename.clone())
        .or_else(|| content_type.name.clone());
    let is_attachment = disposition
        .as_ref()
        .map(|disposition| disposition.disposition.eq_ignore_ascii_case("attachment"))
        .unwrap_or(false)
        || filename.is_some();
    let is_inline = disposition
        .as_ref()
        .map(|disposition| disposition.disposition.eq_ignore_ascii_case("inline"))
        .unwrap_or(false);

    if !is_attachment && content_type.media_type == "text/plain" {
        if collector.plain.is_none() {
            collector.plain = Some(decode_text(&decoded, content_type.charset.as_deref()));
        }
        return Ok(());
    }

    if !is_attachment && content_type.media_type == "text/html" {
        if collector.html.is_none() {
            collector.html = Some(decode_text(&decoded, content_type.charset.as_deref()));
        }
        return Ok(());
    }

    let filename = filename.unwrap_or_else(|| {
        format!(
            "attachment-{}",
            collector.attachments.len().saturating_add(1)
        )
    });
    let id = AttachmentId(format!(
        "attachment:{}:{}",
        collector.attachments.len().saturating_add(1),
        sanitize_attachment_id(&filename)
    ));

    collector.attachments.push(ParsedAttachment {
        id,
        filename,
        mime_type: content_type.media_type,
        content_id: part.content_id(),
        inline: is_inline,
        size: decoded.len() as u64,
        data: decoded,
    });

    Ok(())
}

fn parse_part(input: &str) -> Result<MimePart> {
    let (raw_headers, body) = split_header_body(input).ok_or(Error::MissingBody)?;
    Ok(MimePart {
        headers: parse_headers(raw_headers),
        body: body.to_string(),
    })
}

fn split_header_body(input: &str) -> Option<(&str, &str)> {
    if let Some(index) = input.find("\r\n\r\n") {
        Some((&input[..index], &input[index + 4..]))
    } else {
        input
            .find("\n\n")
            .map(|index| (&input[..index], &input[index + 2..]))
    }
}

fn parse_headers(input: &str) -> Headers {
    let mut headers = Vec::<(String, String)>::new();

    for line in input.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, value)) = headers.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }

        let Some((name, value)) = line.split_once(':') else {
            continue;
        };

        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }

    Headers(headers)
}

fn parsed_headers(headers: &Headers) -> ParsedHeaders {
    ParsedHeaders {
        subject: headers.get("subject").unwrap_or_default().to_string(),
        from: headers.get("from").unwrap_or_default().to_string(),
        to: headers
            .get("to")
            .map(parse_address_list)
            .unwrap_or_default(),
        message_id: headers
            .get("message-id")
            .map(|value| value.trim_matches(['<', '>']).to_string())
            .filter(|value| !value.is_empty()),
        date: headers.get("date").map(ToOwned::to_owned),
    }
}

fn parse_address_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn split_multipart(body: &str, boundary: &str) -> Result<Vec<MimePart>> {
    if boundary.trim().is_empty() {
        return Err(Error::InvalidBoundary(boundary.to_string()));
    }

    let marker = format!("--{boundary}");
    let end_marker = format!("--{boundary}--");
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_part = false;

    for line in body.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed == marker {
            if in_part && !current.trim().is_empty() {
                parts.push(parse_part(current.trim_start_matches(['\r', '\n']))?);
                current.clear();
            }
            in_part = true;
            continue;
        }

        if trimmed == end_marker {
            if in_part && !current.trim().is_empty() {
                parts.push(parse_part(current.trim_start_matches(['\r', '\n']))?);
            }
            return Ok(parts);
        }

        if in_part {
            current.push_str(line);
            current.push('\n');
        }
    }

    if in_part && !current.trim().is_empty() {
        parts.push(parse_part(current.trim_start_matches(['\r', '\n']))?);
    }

    Ok(parts)
}

fn decode_transfer(value: &[u8], encoding: Option<&str>) -> Vec<u8> {
    match encoding.unwrap_or_default().to_ascii_lowercase().as_str() {
        "base64" => decode_base64(value),
        "quoted-printable" => decode_quoted_printable(value),
        _ => normalize_body_bytes(value),
    }
}

fn decode_base64(value: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = Vec::with_capacity(4);

    for byte in value
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
    {
        buffer.push(byte);
        if buffer.len() == 4 {
            decode_base64_chunk(&buffer, &mut output);
            buffer.clear();
        }
    }

    if !buffer.is_empty() {
        while buffer.len() < 4 {
            buffer.push(b'=');
        }
        decode_base64_chunk(&buffer, &mut output);
    }

    output
}

fn decode_base64_chunk(chunk: &[u8], output: &mut Vec<u8>) {
    let mut values = [0u8; 4];
    let mut padding = 0;

    for (index, byte) in chunk.iter().copied().enumerate().take(4) {
        match base64_value(byte) {
            Some(value) => values[index] = value,
            None if byte == b'=' => padding += 1,
            None => return,
        }
    }

    output.push((values[0] << 2) | (values[1] >> 4));
    if padding < 2 {
        output.push((values[1] << 4) | (values[2] >> 2));
    }
    if padding == 0 {
        output.push((values[2] << 6) | values[3]);
    }
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode_quoted_printable(value: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(value.len());
    let mut index = 0;

    while index < value.len() {
        match value[index] {
            b'=' if index + 2 < value.len()
                && (value[index + 1] == b'\r' || value[index + 1] == b'\n') =>
            {
                index += if value[index + 1] == b'\r'
                    && index + 2 < value.len()
                    && value[index + 2] == b'\n'
                {
                    3
                } else {
                    2
                };
            }
            b'=' if index + 2 < value.len() => {
                let hi = hex_value(value[index + 1]);
                let lo = hex_value(value[index + 2]);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    output.push((hi << 4) | lo);
                    index += 3;
                } else {
                    output.push(value[index]);
                    index += 1;
                }
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }

    normalize_body_bytes(&output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn normalize_body_bytes(value: &[u8]) -> Vec<u8> {
    let mut output = value.to_vec();
    while output.ends_with(b"\r\n") || output.ends_with(b"\n") {
        output.pop();
        if output.ends_with(b"\r") {
            output.pop();
        }
    }
    output
}

fn decode_text(value: &[u8], _charset: Option<&str>) -> String {
    String::from_utf8_lossy(value).to_string()
}

fn sanitize_attachment_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

impl MimePart {
    fn content_type(&self) -> ContentType {
        parse_content_type(self.headers.get("content-type").unwrap_or("text/plain"))
    }

    fn content_disposition(&self) -> Option<ContentDisposition> {
        self.headers
            .get("content-disposition")
            .map(parse_content_disposition)
    }

    fn transfer_encoding(&self) -> Option<&str> {
        self.headers.get("content-transfer-encoding")
    }

    fn content_id(&self) -> Option<String> {
        self.headers
            .get("content-id")
            .map(|value| value.trim_matches(['<', '>']).to_string())
    }
}

impl Headers {
    fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .rev()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn parse_content_type(value: &str) -> ContentType {
    let (media_type, params) = parse_parameterized_header(value);
    ContentType {
        media_type: media_type
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "text/plain".to_string())
            .to_ascii_lowercase(),
        boundary: params.get("boundary").cloned(),
        name: params.get("name").cloned(),
        charset: params.get("charset").cloned(),
    }
}

fn parse_content_disposition(value: &str) -> ContentDisposition {
    let (disposition, params) = parse_parameterized_header(value);
    ContentDisposition {
        disposition: disposition.unwrap_or_else(|| "inline".to_string()),
        filename: params
            .get("filename")
            .cloned()
            .or_else(|| params.get("filename*").map(|value| decode_rfc5987(value))),
    }
}

fn parse_parameterized_header(
    value: &str,
) -> (Option<String>, std::collections::BTreeMap<String, String>) {
    let mut segments = split_header_params(value);
    let main = segments.next().map(|value| value.trim().to_string());
    let mut params = std::collections::BTreeMap::new();

    for segment in segments {
        let Some((name, value)) = segment.split_once('=') else {
            continue;
        };
        params.insert(
            name.trim().to_ascii_lowercase(),
            unquote_header_value(value.trim()),
        );
    }

    (main, params)
}

fn split_header_params(value: &str) -> impl Iterator<Item = &str> {
    HeaderParamIter {
        value,
        offset: 0,
        in_quote: false,
    }
}

struct HeaderParamIter<'a> {
    value: &'a str,
    offset: usize,
    in_quote: bool,
}

impl<'a> Iterator for HeaderParamIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.value.len() {
            return None;
        }

        let start = self.offset;
        for (relative, ch) in self.value[start..].char_indices() {
            match ch {
                '"' => self.in_quote = !self.in_quote,
                ';' if !self.in_quote => {
                    let end = start + relative;
                    self.offset = end + 1;
                    return Some(&self.value[start..end]);
                }
                _ => {}
            }
        }

        self.offset = self.value.len();
        Some(&self.value[start..])
    }
}

fn unquote_header_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        value.to_string()
    }
}

fn decode_rfc5987(value: &str) -> String {
    let value = unquote_header_value(value);
    let encoded = value
        .rsplit_once("''")
        .map(|(_, value)| value)
        .unwrap_or(&value);
    percent_decode(encoded)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            output.push((hi << 4) | lo);
            index += 3;
            continue;
        }

        output.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&output).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_quoted_printable_text() {
        let parsed = parse_rfc822(
            b"Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nHello=2C Courier=21\r\n",
        )
        .expect("parse message");

        assert_eq!(parsed.body.kind, BodyKind::PlainText);
        assert_eq!(parsed.headers.subject, "");
        assert_eq!(parsed.body.content, "Hello, Courier!");
        assert!(parsed.attachments.is_empty());
    }

    #[test]
    fn prefers_html_from_multipart_alternative() {
        let raw = br#"Subject: Alternative
Message-ID: <alt@example.test>
To: you@example.test, team@example.test
Content-Type: multipart/alternative; boundary="alt"

--alt
Content-Type: text/plain

Plain body
--alt
Content-Type: text/html

<p>HTML body</p>
--alt--
"#;

        let parsed = parse_rfc822(raw).expect("parse message");

        assert_eq!(parsed.body.kind, BodyKind::Html);
        assert_eq!(parsed.body.content, "<p>HTML body</p>");
        assert_eq!(parsed.headers.subject, "Alternative");
        assert_eq!(
            parsed.headers.message_id,
            Some("alt@example.test".to_string())
        );
        assert_eq!(
            parsed.headers.to,
            vec![
                "you@example.test".to_string(),
                "team@example.test".to_string()
            ]
        );
    }

    #[test]
    fn extracts_attachment_metadata_and_data() {
        let raw = br#"Content-Type: multipart/mixed; boundary="mix"

--mix
Content-Type: text/plain

See attached.
--mix
Content-Type: application/pdf; name="report.pdf"
Content-Disposition: attachment; filename="report.pdf"
Content-Transfer-Encoding: base64

JVBERg==
--mix--
"#;

        let parsed = parse_rfc822(raw).expect("parse message");

        assert_eq!(parsed.body.content, "See attached.");
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.attachments[0].filename, "report.pdf");
        assert_eq!(parsed.attachments[0].mime_type, "application/pdf");
        assert_eq!(parsed.attachments[0].size, 4);
        assert_eq!(parsed.attachments[0].data, b"%PDF");
    }

    #[test]
    fn handles_folded_headers_and_rfc5987_filename() {
        let raw = br#"Content-Type: application/octet-stream
Content-Disposition: attachment;
 filename*=utf-8''report%20final.txt
Content-Transfer-Encoding: base64

SGVsbG8=
"#;

        let parsed = parse_rfc822(raw).expect("parse message");

        assert_eq!(parsed.body.content, "");
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.attachments[0].filename, "report final.txt");
        assert_eq!(parsed.attachments[0].data, b"Hello");
    }
}
