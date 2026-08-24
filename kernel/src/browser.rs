//! Bounded, allocation-free browser primitives.
//!
//! Kept independent from kernel globals so same parser can be fuzzed/tested on
//! host and called by native user-process services. This is not a DOM or a CSS
//! engine: it is a safe HTTP/1.1 + readable-HTML foundation for MakOS Browser.

use core::net::Ipv6Addr;
use core::str;

pub const MAX_URL_BYTES: usize = 1024;
pub const MAX_HOST_BYTES: usize = 253;
pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_HEADERS: usize = 64;
pub const MAX_REDIRECTS: usize = 8;
pub const MAX_LINK_BYTES: usize = 256;
pub const MAX_TITLE_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scheme {
    Http,
    Https,
}

impl Scheme {
    pub const fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostKind {
    NameOrIpv4,
    Ipv6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UrlError {
    Empty,
    TooLong,
    UnsupportedScheme,
    MissingHost,
    InvalidHost,
    InvalidPort,
    CredentialsForbidden,
    ControlCharacter,
    InvalidPercentEscape,
    OutputTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParsedUrl<'a> {
    pub scheme: Scheme,
    /// Brackets excluded for IPv6 literals.
    pub host: &'a str,
    pub host_kind: HostKind,
    pub port: u16,
    /// Empty means `/`; a leading `?` means `/?...`.
    pub path_query: &'a str,
}

pub fn parse_url(input: &str) -> Result<ParsedUrl<'_>, UrlError> {
    if input.is_empty() {
        return Err(UrlError::Empty);
    }
    if input.len() > MAX_URL_BYTES {
        return Err(UrlError::TooLong);
    }
    if input
        .bytes()
        .any(|byte| byte <= 0x20 || byte == 0x7f || !byte.is_ascii())
    {
        return Err(UrlError::ControlCharacter);
    }
    validate_percent_escapes(input.as_bytes())?;

    let (scheme, rest) = if let Some(value) = strip_prefix_ascii_case(input, "http://") {
        (Scheme::Http, value)
    } else if let Some(value) = strip_prefix_ascii_case(input, "https://") {
        (Scheme::Https, value)
    } else {
        return Err(UrlError::UnsupportedScheme);
    };
    let authority_end = rest
        .bytes()
        .position(|byte| matches!(byte, b'/' | b'?' | b'#'))
        .unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() {
        return Err(UrlError::MissingHost);
    }
    if authority.as_bytes().contains(&b'@') {
        return Err(UrlError::CredentialsForbidden);
    }

    let (host, host_kind, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let Some(close) = bracketed.find(']') else {
            return Err(UrlError::InvalidHost);
        };
        let host = &bracketed[..close];
        let suffix = &bracketed[close + 1..];
        if host.is_empty() || host.parse::<Ipv6Addr>().is_err() {
            return Err(UrlError::InvalidHost);
        }
        let port = parse_port_suffix(suffix, scheme.default_port())?;
        (host, HostKind::Ipv6, port)
    } else {
        let (host, suffix) = match authority.rfind(':') {
            Some(index) => {
                if authority[..index].as_bytes().contains(&b':') {
                    return Err(UrlError::InvalidHost);
                }
                (&authority[..index], &authority[index..])
            }
            None => (authority, ""),
        };
        validate_host_name(host)?;
        let port = parse_port_suffix(suffix, scheme.default_port())?;
        (host, HostKind::NameOrIpv4, port)
    };
    let tail = &rest[authority_end..];
    let path_query = tail.split('#').next().unwrap_or("");
    Ok(ParsedUrl {
        scheme,
        host,
        host_kind,
        port,
        path_query,
    })
}

fn parse_port_suffix(suffix: &str, default: u16) -> Result<u16, UrlError> {
    if suffix.is_empty() {
        return Ok(default);
    }
    let Some(value) = suffix.strip_prefix(':') else {
        return Err(UrlError::InvalidPort);
    };
    if value.is_empty() || value.len() > 5 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(UrlError::InvalidPort);
    }
    let mut port = 0u32;
    for byte in value.bytes() {
        port = port * 10 + u32::from(byte - b'0');
    }
    if port == 0 || port > u16::MAX as u32 {
        return Err(UrlError::InvalidPort);
    }
    Ok(port as u16)
}

fn validate_host_name(host: &str) -> Result<(), UrlError> {
    if host.is_empty() || host.len() > MAX_HOST_BYTES {
        return Err(if host.is_empty() {
            UrlError::MissingHost
        } else {
            UrlError::InvalidHost
        });
    }
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.is_empty() {
        return Err(UrlError::InvalidHost);
    }
    for label in host.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(UrlError::InvalidHost);
        }
    }
    Ok(())
}

fn validate_percent_escapes(bytes: &[u8]) -> Result<(), UrlError> {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(UrlError::InvalidPercentEscape);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn strip_prefix_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let candidate = value.get(..prefix.len())?;
    candidate
        .eq_ignore_ascii_case(prefix)
        .then(|| &value[prefix.len()..])
}

struct Writer<'a> {
    bytes: &'a mut [u8],
    used: usize,
}

impl<'a> Writer<'a> {
    fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes, used: 0 }
    }

    fn push(&mut self, value: &[u8]) -> Result<(), UrlError> {
        let end = self
            .used
            .checked_add(value.len())
            .ok_or(UrlError::OutputTooSmall)?;
        let output = self
            .bytes
            .get_mut(self.used..end)
            .ok_or(UrlError::OutputTooSmall)?;
        output.copy_from_slice(value);
        self.used = end;
        Ok(())
    }

    fn push_port(&mut self, port: u16) -> Result<(), UrlError> {
        let mut digits = [0u8; 5];
        let mut cursor = digits.len();
        let mut value = port;
        loop {
            cursor -= 1;
            digits[cursor] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        self.push(&digits[cursor..])
    }
}

pub fn build_http_get(url: ParsedUrl<'_>, output: &mut [u8]) -> Result<usize, UrlError> {
    let mut writer = Writer::new(output);
    writer.push(b"GET ")?;
    if url.path_query.is_empty() {
        writer.push(b"/")?;
    } else if url.path_query.starts_with('?') {
        writer.push(b"/")?;
        writer.push(url.path_query.as_bytes())?;
    } else {
        writer.push(url.path_query.as_bytes())?;
    }
    writer.push(b" HTTP/1.1\r\nHost: ")?;
    if url.host_kind == HostKind::Ipv6 {
        writer.push(b"[")?;
    }
    writer.push(url.host.as_bytes())?;
    if url.host_kind == HostKind::Ipv6 {
        writer.push(b"]")?;
    }
    if url.port != url.scheme.default_port() {
        writer.push(b":")?;
        writer.push_port(url.port)?;
    }
    writer.push(
        b"\r\nUser-Agent: MakOS-Browser/0.1\r\nAccept: text/html,text/plain;q=0.9,*/*;q=0.1\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
    )?;
    Ok(writer.used)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpError {
    Incomplete,
    HeaderTooLarge,
    TooManyHeaders,
    InvalidStatusLine,
    InvalidHeader,
    ConflictingLength,
    UnsupportedTransferEncoding,
    AmbiguousFraming,
    InvalidChunk,
    BodyTooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpVersion {
    Http10,
    Http11,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyKind {
    Identity,
    ContentLength,
    Chunked,
}

#[derive(Clone, Copy, Debug)]
pub struct HttpResponse<'a> {
    pub version: HttpVersion,
    pub status: u16,
    pub reason: &'a [u8],
    pub body: &'a [u8],
    pub body_kind: BodyKind,
    pub content_type: Option<&'a [u8]>,
    pub location: Option<&'a [u8]>,
    headers: &'a [u8],
}

impl<'a> HttpResponse<'a> {
    pub fn header(&self, wanted: &[u8]) -> Option<&'a [u8]> {
        let mut lines = HeaderLines::new(self.headers);
        while let Some((name, value)) = lines.next() {
            if name.eq_ignore_ascii_case(wanted) {
                return Some(value);
            }
        }
        None
    }

    pub const fn is_redirect(&self) -> bool {
        matches!(self.status, 301 | 302 | 303 | 307 | 308)
    }
}

pub fn parse_http_response(input: &[u8]) -> Result<HttpResponse<'_>, HttpError> {
    let Some(header_end) = find_bytes(input, b"\r\n\r\n") else {
        return Err(if input.len() > MAX_HEADER_BYTES {
            HttpError::HeaderTooLarge
        } else {
            HttpError::Incomplete
        });
    };
    if header_end + 4 > MAX_HEADER_BYTES {
        return Err(HttpError::HeaderTooLarge);
    }
    let head = &input[..header_end];
    let status_end = find_bytes(head, b"\r\n").unwrap_or(head.len());
    let status_line = &head[..status_end];
    let (version, status, reason) = parse_status_line(status_line)?;
    let headers = if status_end == head.len() {
        &head[head.len()..]
    } else {
        &head[status_end + 2..]
    };
    let mut lines = HeaderLines::new(headers);
    let mut count = 0usize;
    let mut content_length = None;
    let mut content_type = None;
    let mut location = None;
    let mut chunked = false;
    while let Some((name, value)) = lines.next_checked()? {
        count += 1;
        if count > MAX_HEADERS {
            return Err(HttpError::TooManyHeaders);
        }
        if name.eq_ignore_ascii_case(b"content-length") {
            let parsed = parse_decimal(value).ok_or(HttpError::InvalidHeader)?;
            if content_length.is_some_and(|old| old != parsed) {
                return Err(HttpError::ConflictingLength);
            }
            content_length = Some(parsed);
        } else if name.eq_ignore_ascii_case(b"transfer-encoding") {
            if !value.eq_ignore_ascii_case(b"chunked") {
                return Err(HttpError::UnsupportedTransferEncoding);
            }
            chunked = true;
        } else if name.eq_ignore_ascii_case(b"content-type") {
            content_type = Some(value);
        } else if name.eq_ignore_ascii_case(b"location") {
            location = Some(value);
        }
    }
    if chunked && content_length.is_some() {
        return Err(HttpError::AmbiguousFraming);
    }
    let available = &input[header_end + 4..];
    let (body, body_kind) = if chunked {
        validate_chunked(available)?;
        (available, BodyKind::Chunked)
    } else if let Some(length) = content_length {
        if available.len() < length {
            return Err(HttpError::Incomplete);
        }
        (&available[..length], BodyKind::ContentLength)
    } else {
        (available, BodyKind::Identity)
    };
    Ok(HttpResponse {
        version,
        status,
        reason,
        body,
        body_kind,
        content_type,
        location,
        headers,
    })
}

fn parse_status_line(line: &[u8]) -> Result<(HttpVersion, u16, &[u8]), HttpError> {
    let version = if line.starts_with(b"HTTP/1.1 ") {
        HttpVersion::Http11
    } else if line.starts_with(b"HTTP/1.0 ") {
        HttpVersion::Http10
    } else {
        return Err(HttpError::InvalidStatusLine);
    };
    if line.len() < 12 || !line[9..12].iter().all(u8::is_ascii_digit) {
        return Err(HttpError::InvalidStatusLine);
    }
    let status = u16::from(line[9] - b'0') * 100
        + u16::from(line[10] - b'0') * 10
        + u16::from(line[11] - b'0');
    if !(100..=599).contains(&status) || (line.len() > 12 && line[12] != b' ') {
        return Err(HttpError::InvalidStatusLine);
    }
    let reason = if line.len() > 12 { &line[13..] } else { b"" };
    if reason.iter().any(|byte| *byte < 0x20 && *byte != b'\t') {
        return Err(HttpError::InvalidStatusLine);
    }
    Ok((version, status, reason))
}

struct HeaderLines<'a> {
    remaining: &'a [u8],
}

impl<'a> HeaderLines<'a> {
    const fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }

    fn next(&mut self) -> Option<(&'a [u8], &'a [u8])> {
        self.next_checked().ok().flatten()
    }

    fn next_checked(&mut self) -> Result<Option<(&'a [u8], &'a [u8])>, HttpError> {
        if self.remaining.is_empty() {
            return Ok(None);
        }
        let (line, rest) = if let Some(end) = find_bytes(self.remaining, b"\r\n") {
            (&self.remaining[..end], &self.remaining[end + 2..])
        } else {
            (self.remaining, &self.remaining[self.remaining.len()..])
        };
        self.remaining = rest;
        if line.is_empty() || matches!(line[0], b' ' | b'\t') {
            return Err(HttpError::InvalidHeader);
        }
        let Some(colon) = line.iter().position(|byte| *byte == b':') else {
            return Err(HttpError::InvalidHeader);
        };
        let name = &line[..colon];
        if name.is_empty() || !name.iter().copied().all(is_header_token) {
            return Err(HttpError::InvalidHeader);
        }
        let value = trim_http_whitespace(&line[colon + 1..]);
        if value.iter().any(|byte| *byte < 0x20 && *byte != b'\t') {
            return Err(HttpError::InvalidHeader);
        }
        Ok(Some((name, value)))
    }
}

fn is_header_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn trim_http_whitespace(mut bytes: &[u8]) -> &[u8] {
    while bytes
        .first()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        bytes = &bytes[1..];
    }
    while bytes
        .last()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn parse_decimal(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut value = 0usize;
    for byte in bytes {
        value = value
            .checked_mul(10)?
            .checked_add(usize::from(*byte - b'0'))?;
    }
    Some(value)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn validate_chunked(mut input: &[u8]) -> Result<(), HttpError> {
    loop {
        let Some(line_end) = find_bytes(input, b"\r\n") else {
            return Err(HttpError::Incomplete);
        };
        let size_field = input[..line_end]
            .split(|byte| *byte == b';')
            .next()
            .unwrap_or(&[]);
        let size = parse_hex(size_field).ok_or(HttpError::InvalidChunk)?;
        input = &input[line_end + 2..];
        if size == 0 {
            // Empty trailer or validated header-shaped trailer fields.
            loop {
                let Some(end) = find_bytes(input, b"\r\n") else {
                    return Err(HttpError::Incomplete);
                };
                let line = &input[..end];
                input = &input[end + 2..];
                if line.is_empty() {
                    return Ok(());
                }
                let mut validator = HeaderLines::new(line);
                validator.next_checked()?;
            }
        }
        let end = size.checked_add(2).ok_or(HttpError::BodyTooLarge)?;
        if input.len() < end {
            return Err(HttpError::Incomplete);
        }
        if input[size..end] != *b"\r\n" {
            return Err(HttpError::InvalidChunk);
        }
        input = &input[end..];
    }
}

fn parse_hex(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() || bytes.len() > 16 {
        return None;
    }
    let mut value = 0usize;
    for byte in bytes {
        let digit = match byte {
            b'0'..=b'9' => usize::from(*byte - b'0'),
            b'a'..=b'f' => usize::from(*byte - b'a') + 10,
            b'A'..=b'F' => usize::from(*byte - b'A') + 10,
            _ => return None,
        };
        value = value.checked_mul(16)?.checked_add(digit)?;
    }
    Some(value)
}

pub fn decode_body(response: HttpResponse<'_>, output: &mut [u8]) -> Result<usize, HttpError> {
    if response.body_kind != BodyKind::Chunked {
        if output.len() < response.body.len() {
            return Err(HttpError::BodyTooLarge);
        }
        output[..response.body.len()].copy_from_slice(response.body);
        return Ok(response.body.len());
    }
    let mut input = response.body;
    let mut used = 0usize;
    loop {
        let line_end = find_bytes(input, b"\r\n").ok_or(HttpError::Incomplete)?;
        let size = parse_hex(
            input[..line_end]
                .split(|byte| *byte == b';')
                .next()
                .unwrap_or(&[]),
        )
        .ok_or(HttpError::InvalidChunk)?;
        input = &input[line_end + 2..];
        if size == 0 {
            return Ok(used);
        }
        let end = used.checked_add(size).ok_or(HttpError::BodyTooLarge)?;
        if end > output.len() || size + 2 > input.len() {
            return Err(HttpError::BodyTooLarge);
        }
        output[used..end].copy_from_slice(&input[..size]);
        used = end;
        input = &input[size + 2..];
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockStyle {
    Normal,
    Heading1,
    Heading2,
    Heading3,
    Preformatted,
    ListItem,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutLine {
    pub start: u32,
    pub length: u16,
    pub style: BlockStyle,
}

impl LayoutLine {
    const EMPTY: Self = Self {
        start: 0,
        length: 0,
        style: BlockStyle::Normal,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HtmlLink {
    pub text_start: u32,
    pub text_length: u16,
    target: [u8; MAX_LINK_BYTES],
    target_length: u16,
}

impl HtmlLink {
    const EMPTY: Self = Self {
        text_start: 0,
        text_length: 0,
        target: [0; MAX_LINK_BYTES],
        target_length: 0,
    };

    pub fn target(&self) -> &str {
        str::from_utf8(&self.target[..usize::from(self.target_length)]).unwrap_or("")
    }
}

#[derive(Clone, Copy)]
struct StyleRun {
    start: u32,
    style: BlockStyle,
}

impl StyleRun {
    const EMPTY: Self = Self {
        start: 0,
        style: BlockStyle::Normal,
    };
}

pub struct HtmlDocument<const TEXT: usize, const LINES: usize, const LINKS: usize> {
    text: [u8; TEXT],
    text_length: usize,
    title: [u8; MAX_TITLE_BYTES],
    title_length: usize,
    lines: [LayoutLine; LINES],
    line_count: usize,
    links: [HtmlLink; LINKS],
    link_count: usize,
    styles: [StyleRun; 64],
    style_count: usize,
    pub truncated: bool,
}

impl<const TEXT: usize, const LINES: usize, const LINKS: usize> HtmlDocument<TEXT, LINES, LINKS> {
    pub const fn new() -> Self {
        Self {
            text: [0; TEXT],
            text_length: 0,
            title: [0; MAX_TITLE_BYTES],
            title_length: 0,
            lines: [LayoutLine::EMPTY; LINES],
            line_count: 0,
            links: [HtmlLink::EMPTY; LINKS],
            link_count: 0,
            styles: [StyleRun::EMPTY; 64],
            style_count: 0,
            truncated: false,
        }
    }

    pub fn clear(&mut self) {
        self.text_length = 0;
        self.title_length = 0;
        self.line_count = 0;
        self.link_count = 0;
        self.style_count = 0;
        self.truncated = false;
    }

    pub fn text(&self) -> &str {
        str::from_utf8(&self.text[..self.text_length]).unwrap_or("")
    }

    pub fn title(&self) -> &str {
        str::from_utf8(&self.title[..self.title_length]).unwrap_or("")
    }

    pub fn lines(&self) -> &[LayoutLine] {
        &self.lines[..self.line_count]
    }

    pub fn links(&self) -> &[HtmlLink] {
        &self.links[..self.link_count]
    }

    pub fn line_text(&self, line: LayoutLine) -> &str {
        let start = line.start as usize;
        let end = start + usize::from(line.length);
        str::from_utf8(&self.text[start..end]).unwrap_or("")
    }

    fn push_byte(&mut self, byte: u8) {
        if self.text_length == TEXT {
            self.truncated = true;
            return;
        }
        self.text[self.text_length] = byte;
        self.text_length += 1;
    }

    fn newline(&mut self) {
        while self.text_length > 0 && self.text[self.text_length - 1] == b' ' {
            self.text_length -= 1;
        }
        if self.text_length > 0 && self.text[self.text_length - 1] != b'\n' {
            self.push_byte(b'\n');
        }
    }

    fn begin_style(&mut self, style: BlockStyle) {
        if self.style_count < self.styles.len() {
            self.styles[self.style_count] = StyleRun {
                start: self.text_length as u32,
                style,
            };
            self.style_count += 1;
        } else {
            self.truncated = true;
        }
    }

    fn style_at(&self, offset: usize) -> BlockStyle {
        let mut result = BlockStyle::Normal;
        for run in &self.styles[..self.style_count] {
            if run.start as usize > offset {
                break;
            }
            result = run.style;
        }
        result
    }

    fn layout(&mut self, columns: usize) {
        self.line_count = 0;
        let columns = columns.clamp(8, u16::MAX as usize);
        let mut start = 0usize;
        while start < self.text_length {
            while start < self.text_length && self.text[start] == b'\n' {
                start += 1;
            }
            if start == self.text_length {
                break;
            }
            let hard_end = self.text[start..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(self.text_length, |offset| start + offset);
            let mut line_end = hard_end.min(start + columns);
            if line_end < hard_end {
                if let Some(space) = self.text[start..line_end]
                    .iter()
                    .rposition(|byte| *byte == b' ')
                {
                    if space > 0 {
                        line_end = start + space;
                    }
                }
            }
            while line_end > start && self.text[line_end - 1] == b' ' {
                line_end -= 1;
            }
            if line_end == start {
                line_end = (start + columns).min(hard_end);
            }
            if self.line_count == LINES {
                self.truncated = true;
                return;
            }
            self.lines[self.line_count] = LayoutLine {
                start: start as u32,
                length: (line_end - start) as u16,
                style: self.style_at(start),
            };
            self.line_count += 1;
            start = line_end;
            while start < hard_end && self.text[start] == b' ' {
                start += 1;
            }
            if start == hard_end {
                start += usize::from(start < self.text_length && self.text[start] == b'\n');
            }
        }
    }
}

impl<const TEXT: usize, const LINES: usize, const LINKS: usize> Default
    for HtmlDocument<TEXT, LINES, LINKS>
{
    fn default() -> Self {
        Self::new()
    }
}

pub fn render_html<const TEXT: usize, const LINES: usize, const LINKS: usize>(
    input: &[u8],
    viewport_columns: usize,
    document: &mut HtmlDocument<TEXT, LINES, LINKS>,
) {
    document.clear();
    let mut index = 0usize;
    let mut hidden_depth = 0usize;
    let mut in_title = false;
    let mut preformatted = false;
    let mut pending_space = false;
    let mut open_link: Option<(usize, [u8; MAX_LINK_BYTES], usize)> = None;
    while index < input.len() {
        if input[index] == b'<' {
            if input[index..].starts_with(b"<!--") {
                index = find_bytes(&input[index + 4..], b"-->")
                    .map_or(input.len(), |offset| index + 4 + offset + 3);
                continue;
            }
            let Some(relative_end) = find_tag_end(&input[index + 1..]) else {
                document.truncated = true;
                break;
            };
            let tag = &input[index + 1..index + 1 + relative_end];
            index += relative_end + 2;
            let tag = trim_ascii(tag);
            if tag.is_empty() || matches!(tag[0], b'!' | b'?') {
                continue;
            }
            let closing = tag[0] == b'/';
            let body = trim_ascii(if closing { &tag[1..] } else { tag });
            let name_end = body
                .iter()
                .position(|byte| byte.is_ascii_whitespace() || *byte == b'/')
                .unwrap_or(body.len());
            let name = &body[..name_end];
            let attributes = &body[name_end..];
            if equals_ascii_case(name, b"script") || equals_ascii_case(name, b"style") {
                if closing {
                    hidden_depth = hidden_depth.saturating_sub(1);
                } else {
                    hidden_depth = hidden_depth.saturating_add(1);
                }
                continue;
            }
            if equals_ascii_case(name, b"head") {
                if closing {
                    hidden_depth = hidden_depth.saturating_sub(1);
                } else {
                    hidden_depth = hidden_depth.saturating_add(1);
                }
                continue;
            }
            if equals_ascii_case(name, b"title") {
                in_title = !closing;
                // Title inside head remains captured despite hidden state.
                continue;
            }
            if hidden_depth > 0 {
                continue;
            }
            if is_block_tag(name) || equals_ascii_case(name, b"br") {
                document.newline();
                pending_space = false;
            }
            if !closing {
                let style = if equals_ascii_case(name, b"h1") {
                    BlockStyle::Heading1
                } else if equals_ascii_case(name, b"h2") {
                    BlockStyle::Heading2
                } else if equals_ascii_case(name, b"h3") {
                    BlockStyle::Heading3
                } else if equals_ascii_case(name, b"pre") {
                    preformatted = true;
                    BlockStyle::Preformatted
                } else if equals_ascii_case(name, b"li") {
                    document.push_byte(b'-');
                    document.push_byte(b' ');
                    BlockStyle::ListItem
                } else {
                    BlockStyle::Normal
                };
                if is_block_tag(name) {
                    document.begin_style(style);
                }
                if equals_ascii_case(name, b"a") && open_link.is_none() {
                    if let Some(target) = attribute_value(attributes, b"href") {
                        let mut stored = [0u8; MAX_LINK_BYTES];
                        let count = target.len().min(stored.len());
                        if target[..count]
                            .iter()
                            .all(|byte| byte.is_ascii() && *byte >= 0x20 && *byte != 0x7f)
                        {
                            stored[..count].copy_from_slice(&target[..count]);
                            open_link = Some((document.text_length, stored, count));
                            if count != target.len() {
                                document.truncated = true;
                            }
                        }
                    }
                }
            } else {
                if equals_ascii_case(name, b"pre") {
                    preformatted = false;
                }
                if equals_ascii_case(name, b"a") {
                    if let Some((start, target, count)) = open_link.take() {
                        if document.link_count < LINKS && document.text_length > start {
                            document.links[document.link_count] = HtmlLink {
                                text_start: start as u32,
                                text_length: (document.text_length - start).min(u16::MAX as usize)
                                    as u16,
                                target,
                                target_length: count as u16,
                            };
                            document.link_count += 1;
                        } else if document.text_length > start {
                            document.truncated = true;
                        }
                    }
                }
            }
            continue;
        }
        let (decoded, consumed) = decode_html_character(&input[index..]);
        index += consumed;
        if in_title {
            if decoded.is_ascii_whitespace() {
                if document.title_length > 0
                    && document.title[document.title_length - 1] != b' '
                    && document.title_length < document.title.len()
                {
                    document.title[document.title_length] = b' ';
                    document.title_length += 1;
                }
            } else if document.title_length < document.title.len() {
                document.title[document.title_length] = decoded;
                document.title_length += 1;
            } else {
                document.truncated = true;
            }
            continue;
        }
        if hidden_depth > 0 {
            continue;
        }
        if preformatted {
            document.push_byte(if decoded == b'\r' { b'\n' } else { decoded });
        } else if decoded.is_ascii_whitespace() {
            pending_space =
                document.text_length > 0 && document.text[document.text_length - 1] != b'\n';
        } else {
            if pending_space {
                document.push_byte(b' ');
            }
            pending_space = false;
            document.push_byte(decoded);
        }
    }
    while document.text_length > 0
        && matches!(document.text[document.text_length - 1], b' ' | b'\n')
    {
        document.text_length -= 1;
    }
    document.layout(viewport_columns);
}

fn find_tag_end(input: &[u8]) -> Option<usize> {
    let mut quote = 0u8;
    for (index, byte) in input.iter().copied().enumerate().take(512) {
        if quote == 0 && matches!(byte, b'\'' | b'"') {
            quote = byte;
        } else if quote == byte {
            quote = 0;
        } else if quote == 0 && byte == b'>' {
            return Some(index);
        }
    }
    None
}

fn trim_ascii(mut input: &[u8]) -> &[u8] {
    while input.first().is_some_and(u8::is_ascii_whitespace) {
        input = &input[1..];
    }
    while input.last().is_some_and(u8::is_ascii_whitespace) {
        input = &input[..input.len() - 1];
    }
    input
}

fn equals_ascii_case(left: &[u8], right: &[u8]) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn is_block_tag(name: &[u8]) -> bool {
    [
        b"p".as_slice(),
        b"div",
        b"section",
        b"article",
        b"header",
        b"footer",
        b"nav",
        b"h1",
        b"h2",
        b"h3",
        b"h4",
        b"h5",
        b"h6",
        b"ul",
        b"ol",
        b"li",
        b"pre",
        b"blockquote",
        b"table",
        b"tr",
    ]
    .iter()
    .any(|candidate| equals_ascii_case(name, candidate))
}

fn attribute_value<'a>(mut attributes: &'a [u8], wanted: &[u8]) -> Option<&'a [u8]> {
    loop {
        attributes = trim_ascii(attributes);
        if attributes.is_empty() || attributes[0] == b'/' {
            return None;
        }
        let name_end = attributes
            .iter()
            .position(|byte| byte.is_ascii_whitespace() || *byte == b'=')
            .unwrap_or(attributes.len());
        let name = &attributes[..name_end];
        attributes = &attributes[name_end..];
        while attributes.first().is_some_and(u8::is_ascii_whitespace) {
            attributes = &attributes[1..];
        }
        if attributes.first() != Some(&b'=') {
            continue;
        }
        attributes = &attributes[1..];
        while attributes.first().is_some_and(u8::is_ascii_whitespace) {
            attributes = &attributes[1..];
        }
        let (value, rest) = if matches!(attributes.first(), Some(b'\'' | b'"')) {
            let quote = attributes[0];
            let after = &attributes[1..];
            let end = after.iter().position(|byte| *byte == quote)?;
            (&after[..end], &after[end + 1..])
        } else {
            let end = attributes
                .iter()
                .position(|byte| byte.is_ascii_whitespace() || *byte == b'>')
                .unwrap_or(attributes.len());
            (&attributes[..end], &attributes[end..])
        };
        if equals_ascii_case(name, wanted) {
            return Some(value);
        }
        attributes = rest;
    }
}

fn decode_html_character(input: &[u8]) -> (u8, usize) {
    if input.first() != Some(&b'&') {
        let byte = input[0];
        return (if byte.is_ascii() { byte } else { b'?' }, 1);
    }
    let limit = input.len().min(16);
    let Some(end) = input[..limit].iter().position(|byte| *byte == b';') else {
        return (b'&', 1);
    };
    let entity = &input[1..end];
    let value = if entity.eq_ignore_ascii_case(b"amp") {
        Some(b'&')
    } else if entity.eq_ignore_ascii_case(b"lt") {
        Some(b'<')
    } else if entity.eq_ignore_ascii_case(b"gt") {
        Some(b'>')
    } else if entity.eq_ignore_ascii_case(b"quot") {
        Some(b'"')
    } else if entity.eq_ignore_ascii_case(b"apos") {
        Some(b'\'')
    } else if entity.eq_ignore_ascii_case(b"nbsp") {
        Some(b' ')
    } else if let Some(decimal) = entity.strip_prefix(b"#") {
        let number = if let Some(hex) = decimal
            .strip_prefix(b"x")
            .or_else(|| decimal.strip_prefix(b"X"))
        {
            parse_hex(hex)
        } else {
            parse_decimal(decimal)
        };
        number
            .and_then(|number| u8::try_from(number).ok())
            .filter(u8::is_ascii)
    } else {
        None
    };
    value.map_or((b'&', 1), |byte| (byte, end + 1))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedUrl<const N: usize> {
    bytes: [u8; N],
    length: usize,
}

/// Resolve an HTTP link without heap allocation. Dot segments are removed,
/// fragments are excluded from network/history URLs, and authority never comes
/// from a relative reference unless the reference starts with `//`.
pub fn resolve_url<const N: usize>(base: &str, reference: &str) -> Result<FixedUrl<N>, UrlError> {
    let base = parse_url(base)?;
    if reference.len() > MAX_URL_BYTES {
        return Err(UrlError::TooLong);
    }
    if reference
        .bytes()
        .any(|byte| byte <= 0x20 || byte == 0x7f || !byte.is_ascii() || byte == b'\\')
    {
        return Err(UrlError::ControlCharacter);
    }
    validate_percent_escapes(reference.as_bytes())?;
    let reference = reference.split('#').next().unwrap_or("");
    if strip_prefix_ascii_case(reference, "http://").is_some()
        || strip_prefix_ascii_case(reference, "https://").is_some()
    {
        return FixedUrl::new(reference);
    }

    let mut output = [0u8; N];
    let mut writer = Writer::new(&mut output);
    writer.push(base.scheme.name().as_bytes())?;
    writer.push(b"://")?;
    if base.host_kind == HostKind::Ipv6 {
        writer.push(b"[")?;
    }
    writer.push(base.host.as_bytes())?;
    if base.host_kind == HostKind::Ipv6 {
        writer.push(b"]")?;
    }
    if base.port != base.scheme.default_port() {
        writer.push(b":")?;
        writer.push_port(base.port)?;
    }
    if let Some(authority) = reference.strip_prefix("//") {
        // `scheme:` plus network-path reference; full parser validates authority.
        let mut absolute = [0u8; N];
        let mut absolute_writer = Writer::new(&mut absolute);
        absolute_writer.push(base.scheme.name().as_bytes())?;
        absolute_writer.push(b":")?;
        absolute_writer.push(b"//")?;
        absolute_writer.push(authority.as_bytes())?;
        let absolute_length = absolute_writer.used;
        drop(absolute_writer);
        let value =
            str::from_utf8(&absolute[..absolute_length]).map_err(|_| UrlError::ControlCharacter)?;
        return FixedUrl::new(value);
    }

    let base_target = if base.path_query.is_empty() {
        "/"
    } else if base.path_query.starts_with('?') {
        "/"
    } else {
        base.path_query
    };
    let base_path = base_target.split('?').next().unwrap_or("/");
    let mut raw = [0u8; N];
    let mut raw_writer = Writer::new(&mut raw);
    if reference.is_empty() {
        if base.path_query.is_empty() {
            raw_writer.push(b"/")?;
        } else if base.path_query.starts_with('?') {
            raw_writer.push(b"/")?;
            raw_writer.push(base.path_query.as_bytes())?;
        } else {
            raw_writer.push(base.path_query.as_bytes())?;
        }
    } else if reference.starts_with('?') {
        raw_writer.push(base_path.as_bytes())?;
        raw_writer.push(reference.as_bytes())?;
    } else if reference.starts_with('/') {
        raw_writer.push(reference.as_bytes())?;
    } else {
        let directory_end = base_path.rfind('/').map_or(0, |index| index + 1);
        raw_writer.push(&base_path.as_bytes()[..directory_end])?;
        raw_writer.push(reference.as_bytes())?;
    }
    let raw_length = raw_writer.used;
    drop(raw_writer);
    let mut normalized = [0u8; N];
    let normalized_length = normalize_path_query(&raw[..raw_length], &mut normalized)?;
    writer.push(&normalized[..normalized_length])?;
    let length = writer.used;
    let value = str::from_utf8(&output[..length]).map_err(|_| UrlError::ControlCharacter)?;
    FixedUrl::new(value)
}

fn normalize_path_query(input: &[u8], output: &mut [u8]) -> Result<usize, UrlError> {
    let query = input.iter().position(|byte| *byte == b'?');
    let path_end = query.unwrap_or(input.len());
    let path = &input[..path_end];
    if !path.starts_with(b"/") {
        return Err(UrlError::ControlCharacter);
    }
    let trailing_slash = path.ends_with(b"/") || path.ends_with(b"/.") || path.ends_with(b"/..");
    let mut used = 1usize;
    if output.is_empty() {
        return Err(UrlError::OutputTooSmall);
    }
    output[0] = b'/';
    for segment in path[1..].split(|byte| *byte == b'/') {
        if segment.is_empty() || segment == b"." {
            continue;
        }
        if segment == b".." {
            if used > 1 {
                used -= 1;
                while used > 1 && output[used - 1] != b'/' {
                    used -= 1;
                }
            }
            continue;
        }
        if used > 1 && output[used - 1] != b'/' {
            *output.get_mut(used).ok_or(UrlError::OutputTooSmall)? = b'/';
            used += 1;
        }
        let end = used
            .checked_add(segment.len())
            .ok_or(UrlError::OutputTooSmall)?;
        output
            .get_mut(used..end)
            .ok_or(UrlError::OutputTooSmall)?
            .copy_from_slice(segment);
        used = end;
    }
    if trailing_slash && used > 1 && output[used - 1] != b'/' {
        *output.get_mut(used).ok_or(UrlError::OutputTooSmall)? = b'/';
        used += 1;
    }
    if let Some(query) = query {
        let suffix = &input[query..];
        let end = used
            .checked_add(suffix.len())
            .ok_or(UrlError::OutputTooSmall)?;
        output
            .get_mut(used..end)
            .ok_or(UrlError::OutputTooSmall)?
            .copy_from_slice(suffix);
        used = end;
    }
    Ok(used)
}

impl<const N: usize> FixedUrl<N> {
    pub const EMPTY: Self = Self {
        bytes: [0; N],
        length: 0,
    };

    pub fn new(value: &str) -> Result<Self, UrlError> {
        parse_url(value)?;
        if value.len() > N {
            return Err(UrlError::OutputTooSmall);
        }
        let mut result = Self::EMPTY;
        result.bytes[..value.len()].copy_from_slice(value.as_bytes());
        result.length = value.len();
        Ok(result)
    }

    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.bytes[..self.length]).unwrap_or("")
    }
}

pub struct NavigationHistory<const URL: usize, const SLOTS: usize> {
    entries: [FixedUrl<URL>; SLOTS],
    length: usize,
    cursor: usize,
}

impl<const URL: usize, const SLOTS: usize> NavigationHistory<URL, SLOTS> {
    pub const fn new() -> Self {
        Self {
            entries: [FixedUrl::EMPTY; SLOTS],
            length: 0,
            cursor: 0,
        }
    }

    pub fn navigate(&mut self, value: &str) -> Result<(), UrlError> {
        let value = FixedUrl::new(value)?;
        if self
            .current()
            .is_some_and(|current| current == value.as_str())
        {
            return Ok(());
        }
        if SLOTS == 0 {
            return Err(UrlError::OutputTooSmall);
        }
        if self.length > 0 {
            self.length = self.cursor + 1;
        }
        if self.length == SLOTS {
            self.entries.copy_within(1..SLOTS, 0);
            self.length -= 1;
        }
        self.entries[self.length] = value;
        self.length += 1;
        self.cursor = self.length - 1;
        Ok(())
    }

    pub fn current(&self) -> Option<&str> {
        (self.length > 0).then(|| self.entries[self.cursor].as_str())
    }

    pub fn back(&mut self) -> Option<&str> {
        if self.cursor == 0 || self.length == 0 {
            return None;
        }
        self.cursor -= 1;
        self.current()
    }

    pub fn forward(&mut self) -> Option<&str> {
        if self.length == 0 || self.cursor + 1 >= self.length {
            return None;
        }
        self.cursor += 1;
        self.current()
    }

    pub const fn can_go_back(&self) -> bool {
        self.length > 0 && self.cursor > 0
    }

    pub const fn can_go_forward(&self) -> bool {
        self.length > 0 && self.cursor + 1 < self.length
    }
}

impl<const URL: usize, const SLOTS: usize> Default for NavigationHistory<URL, SLOTS> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_urls_and_builds_origin_form_request() {
        let url = parse_url("http://example.com:8080/a?q=1#ignored").unwrap();
        assert_eq!(url.host, "example.com");
        assert_eq!(url.port, 8080);
        assert_eq!(url.path_query, "/a?q=1");
        let mut output = [0u8; 512];
        let count = build_http_get(url, &mut output).unwrap();
        let request = str::from_utf8(&output[..count]).unwrap();
        assert!(request.starts_with("GET /a?q=1 HTTP/1.1\r\n"));
        assert!(request.contains("Host: example.com:8080\r\n"));
        assert!(!request.contains("ignored"));
    }

    #[test]
    fn rejects_url_injection_credentials_and_bad_ports() {
        assert_eq!(
            parse_url("http://good.test/%0d%0aX:bad"),
            // Encoded CRLF remains data; request target never decodes it.
            Ok(ParsedUrl {
                scheme: Scheme::Http,
                host: "good.test",
                host_kind: HostKind::NameOrIpv4,
                port: 80,
                path_query: "/%0d%0aX:bad",
            })
        );
        assert_eq!(
            parse_url("http://a@b/"),
            Err(UrlError::CredentialsForbidden)
        );
        assert_eq!(parse_url("http://test:0/"), Err(UrlError::InvalidPort));
        assert_eq!(
            parse_url("file:///etc/passwd"),
            Err(UrlError::UnsupportedScheme)
        );
        assert_eq!(
            parse_url("http://bad host/"),
            Err(UrlError::ControlCharacter)
        );
    }

    #[test]
    fn parses_content_length_and_headers() {
        let bytes = b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 5\r\nX-Test: yes\r\n\r\nhelloTRAIL";
        let response = parse_http_response(bytes).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"hello");
        assert_eq!(response.header(b"x-test"), Some(b"yes".as_slice()));
        assert_eq!(
            response.content_type,
            Some(b"text/html; charset=utf-8".as_slice())
        );
    }

    #[test]
    fn accepts_headerless_connection_close_response() {
        let response = parse_http_response(b"HTTP/1.0 204 No Content\r\n\r\n").unwrap();
        assert_eq!(response.status, 204);
        assert_eq!(response.body, b"");
        assert_eq!(response.body_kind, BodyKind::Identity);
    }

    #[test]
    fn decodes_chunked_body_and_rejects_smuggling_shape() {
        let bytes = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5;ext=x\r\npedia\r\n0\r\nX-End: yes\r\n\r\n";
        let response = parse_http_response(bytes).unwrap();
        let mut output = [0u8; 16];
        let count = decode_body(response, &mut output).unwrap();
        assert_eq!(&output[..count], b"Wikipedia");
        assert!(matches!(
            parse_http_response(
                b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
            ),
            Err(HttpError::AmbiguousFraming)
        ));
    }

    #[test]
    fn renders_readable_html_title_links_and_bounded_lines() {
        let html = br#"<!doctype html><html><head><title>MakOS &amp; Web</title><style>bad</style></head><body><h1>Welcome</h1><p>Words inside boxes must wrap safely.</p><script>evil()</script><ul><li><a href="/next">Next page</a></li></ul></body></html>"#;
        let mut document = HtmlDocument::<512, 64, 8>::new();
        render_html(html, 12, &mut document);
        assert_eq!(document.title(), "MakOS & Web");
        assert!(document.text().contains("Welcome"));
        assert!(!document.text().contains("evil"));
        assert_eq!(document.links()[0].target(), "/next");
        assert!(document.lines().iter().all(|line| line.length <= 12));
        assert!(
            document
                .lines()
                .iter()
                .any(|line| line.style == BlockStyle::Heading1)
        );
    }

    #[test]
    fn hard_wraps_long_words_inside_viewport() {
        let mut document = HtmlDocument::<128, 16, 1>::new();
        render_html(b"<p>abcdefghijklmnopqrstuvwxyz</p>", 8, &mut document);
        assert_eq!(document.lines().len(), 4);
        assert!(document.lines().iter().all(|line| line.length <= 8));
    }

    #[test]
    fn navigation_discards_forward_entries_and_evicts_oldest() {
        let mut history = NavigationHistory::<64, 3>::new();
        history.navigate("http://one.test/").unwrap();
        history.navigate("http://two.test/").unwrap();
        history.navigate("http://three.test/").unwrap();
        assert_eq!(history.back(), Some("http://two.test/"));
        history.navigate("http://branch.test/").unwrap();
        assert!(!history.can_go_forward());
        history.navigate("http://four.test/").unwrap();
        assert_eq!(history.back(), Some("http://branch.test/"));
        assert_eq!(history.back(), Some("http://two.test/"));
        assert_eq!(history.back(), None);
    }

    #[test]
    fn resolves_relative_redirects_and_removes_dot_segments() {
        assert_eq!(
            resolve_url::<128>("http://example.test/a/b/index.html?old=1", "../next?q=2#x")
                .unwrap()
                .as_str(),
            "http://example.test/a/next?q=2"
        );
        assert_eq!(
            resolve_url::<128>("https://example.test/a", "//cdn.test/x")
                .unwrap()
                .as_str(),
            "https://cdn.test/x"
        );
        assert_eq!(
            resolve_url::<128>("http://example.test/a/b", "?new=1")
                .unwrap()
                .as_str(),
            "http://example.test/a/b?new=1"
        );
    }
}
