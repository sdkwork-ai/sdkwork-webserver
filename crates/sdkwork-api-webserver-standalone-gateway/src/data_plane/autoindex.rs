//! nginx `autoindex` listing renderer (`html`, `json`, `jsonp`, `xml`).
//!
//! Every byte shape here was differential-probed against nginx 1.29.6
//! (`ngx_http_autoindex_module`): directory entries sort directories first and
//! then by byte name; hidden entries (a leading `.`) are skipped; `html`
//! escapes the href as a URI component and the display name as HTML while
//! padding the name column to nginx's `NGX_HTTP_AUTOINDEX_NAME_LEN` (50) and
//! rendering `DD-Mon-YYYY HH:MM` mtimes plus an exact or human-readable size;
//! `json` renders the `{ "name", "type", "mtime", "size" }` array (with the
//! optional JSONP callback wrapper); `xml` renders the
//! `<directory|file|other mtime="…Z" [size="…"]>name</…>` list. `xml`
//! publishes `charset=utf-8` exactly like nginx.
//!
//! The renderer is bounded: at most [`MAXIMUM_AUTOINDEX_ENTRIES`] entries are
//! collected (a larger directory is answered 403 instead of building an
//! unbounded body), and every produced body is length-prefixed by the caller.

use sdkwork_webserver_core::{AutoindexConfig, AutoindexFormat};

use super::static_path::AutoindexEntry;

/// Upper bound on rendered entries (defence in depth: real nginx has none).
pub(crate) const MAXIMUM_AUTOINDEX_ENTRIES: usize = 4096;

/// nginx `NGX_HTTP_AUTOINDEX_NAME_LEN`: the display-name column width.
const NAME_LEN: usize = 50;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub(crate) struct RenderedAutoindex {
    pub(crate) content_type: &'static str,
    pub(crate) body: Vec<u8>,
}

/// Render a listing for `uri` (the request path, used in the HTML title).
pub(crate) fn render(
    policy: &AutoindexConfig,
    uri: &str,
    entries: &mut [AutoindexEntry],
    callback: Option<&str>,
) -> RenderedAutoindex {
    // nginx sorts directories first, then by byte name (`ngx_strcmp`).
    entries.sort_by(|left, right| match (left.is_dir, right.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => left.name.as_bytes().cmp(right.name.as_bytes()),
    });
    match policy.format {
        AutoindexFormat::Html => RenderedAutoindex {
            content_type: "text/html",
            body: render_html(policy, uri, entries),
        },
        AutoindexFormat::Json => RenderedAutoindex {
            content_type: "application/json",
            body: render_json(entries, None),
        },
        AutoindexFormat::Jsonp => RenderedAutoindex {
            content_type: "application/javascript",
            body: render_json(entries, callback),
        },
        AutoindexFormat::Xml => RenderedAutoindex {
            // nginx sets `headers_out.charset = "utf-8"`, so the wire header is
            // `text/xml; charset=utf-8`.
            content_type: "text/xml; charset=utf-8",
            body: render_xml(entries),
        },
    }
}

/// Validate a JSONP `callback` query argument exactly like
/// `ngx_http_autoindex_jsonp_callback`: 1-128 characters of `[A-Za-z0-9_.]`.
/// Returns `None` for an invalid callback (nginx answers 400).
pub(crate) fn validate_jsonp_callback(callback: &str) -> Option<&str> {
    if callback.is_empty() || callback.len() > 128 {
        return None;
    }
    if callback
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.')
    {
        Some(callback)
    } else {
        None
    }
}

fn render_html(policy: &AutoindexConfig, uri: &str, entries: &[AutoindexEntry]) -> Vec<u8> {
    let escaped_uri = html_escape(uri);
    let mut out = String::with_capacity(256 + entries.len() * 96);
    out.push_str("<html>\r\n<head><title>Index of ");
    out.push_str(&escaped_uri);
    out.push_str("</title></head>\r\n<body>\r\n<h1>Index of ");
    out.push_str(&escaped_uri);
    out.push_str("</h1>");
    out.push_str("<hr><pre><a href=\"../\">../</a>\r\n");
    for entry in entries {
        out.push_str("<a href=\"");
        out.push_str(&uri_escape(&entry.name));
        if entry.is_dir {
            out.push('/');
        }
        out.push_str("\">");
        let char_len = entry.name.chars().count();
        if char_len > NAME_LEN {
            // nginx truncates the display name and marks it with `..>`.
            let truncated: String = entry.name.chars().take(NAME_LEN - 3).collect();
            out.push_str(&html_escape(&truncated));
            out.push_str("..&gt;</a>");
        } else {
            out.push_str(&html_escape(&entry.name));
            let mut padded_len = char_len;
            if entry.is_dir {
                out.push('/');
                padded_len += 1;
            }
            out.push_str("</a>");
            if padded_len < NAME_LEN {
                out.push_str(&" ".repeat(NAME_LEN - padded_len));
            }
        }
        out.push(' ');
        let (year, month, day, hour, minute, second) = broken_down(entry.mtime, policy.localtime);
        out.push_str(&format!(
            "{day:02}-{}-{year:04} {hour:02}:{minute:02} ",
            MONTHS[month as usize - 1]
        ));
        let _ = second;
        if policy.exact_size {
            if entry.is_dir {
                out.push_str("                  -");
            } else {
                out.push_str(&format!("{:>19}", entry.size));
            }
        } else if entry.is_dir {
            out.push_str("      -");
        } else {
            out.push_str(&human_size(entry.size));
        }
        out.push_str("\r\n");
    }
    out.push_str("</pre><hr></body>\r\n</html>\r\n");
    out.into_bytes()
}

/// nginx `autoindex_exact_size off` human-readable size (K/M/G, rounded).
fn human_size(length: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if length > GIB - 1 {
        let mut size = length / GIB;
        if length % GIB > GIB / 2 - 1 {
            size += 1;
        }
        format!("{size:>6}G")
    } else if length > MIB - 1 {
        let mut size = length / MIB;
        if length % MIB > MIB / 2 - 1 {
            size += 1;
        }
        format!("{size:>6}M")
    } else if length > 9999 {
        let mut size = length / 1024;
        if length % 1024 > 511 {
            size += 1;
        }
        format!("{size:>6}K")
    } else {
        format!(" {:>6}", length)
    }
}

fn render_json(entries: &[AutoindexEntry], callback: Option<&str>) -> Vec<u8> {
    let mut out = String::with_capacity(64 + entries.len() * 80);
    if let Some(callback) = callback {
        out.push_str("/* callback */\r\n");
        out.push_str(callback);
        out.push('(');
    }
    out.push('[');
    for entry in entries {
        out.push_str("\r\n{ \"name\":\"");
        out.push_str(&json_escape(&entry.name));
        out.push_str("\", \"type\":\"");
        out.push_str(entry_type(entry));
        out.push_str("\", \"mtime\":\"");
        out.push_str(&http_date(entry.mtime));
        if entry.is_file {
            out.push_str("\", \"size\":");
            out.push_str(&entry.size.to_string());
        } else {
            out.push('"');
        }
        out.push_str(" },");
    }
    if !entries.is_empty() {
        out.pop();
    }
    out.push_str("\r\n]");
    if callback.is_some() {
        out.push_str(");");
    }
    out.into_bytes()
}

fn render_xml(entries: &[AutoindexEntry]) -> Vec<u8> {
    let mut out = String::with_capacity(64 + entries.len() * 96);
    out.push_str("<?xml version=\"1.0\"?>\r\n<list>\r\n");
    for entry in entries {
        let kind = entry_type(entry);
        out.push('<');
        out.push_str(kind);
        out.push_str(" mtime=\"");
        out.push_str(&iso_date(entry.mtime));
        if entry.is_file {
            out.push_str("\" size=\"");
            out.push_str(&entry.size.to_string());
        }
        out.push_str("\">");
        out.push_str(&html_escape(&entry.name));
        out.push_str("</");
        out.push_str(kind);
        out.push_str(">\r\n");
    }
    out.push_str("</list>\r\n");
    out.into_bytes()
}

fn entry_type(entry: &AutoindexEntry) -> &'static str {
    if entry.is_dir {
        "directory"
    } else if entry.is_file {
        "file"
    } else {
        "other"
    }
}

/// nginx `ngx_http_time`: `Wed, 31 Dec 1986 10:00:00 GMT`.
fn http_date(mtime: i64) -> String {
    const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let (year, month, day, hour, minute, second, weekday) = civil_from_unix(mtime);
    format!(
        "{}, {day:02} {} {year:04} {hour:02}:{minute:02}:{second:02} GMT",
        WEEKDAYS[weekday as usize],
        MONTHS[month as usize - 1]
    )
}

/// `YYYY-MM-DDTHH:MM:SSZ` for the XML format.
fn iso_date(mtime: i64) -> String {
    let (year, month, day, hour, minute, second, _weekday) = civil_from_unix(mtime);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Broken-down UTC (or local, with `localtime`) timestamp.
fn broken_down(mtime: i64, localtime: bool) -> (i32, u32, u32, u32, u32, u32) {
    let offset_seconds = if localtime { local_offset_seconds() } else { 0 };
    let (year, month, day, hour, minute, second, _weekday) =
        civil_from_unix(mtime.saturating_add(offset_seconds));
    (year, month, day, hour, minute, second)
}

/// nginx `ngx_gmtime`-style civil conversion from a Unix timestamp.
fn civil_from_unix(timestamp: i64) -> (i32, u32, u32, u32, u32, u32, u32) {
    let days = timestamp.div_euclid(86_400);
    let seconds_of_day = timestamp.rem_euclid(86_400);
    let hour = (seconds_of_day / 3600) as u32;
    let minute = ((seconds_of_day % 3600) / 60) as u32;
    let second = (seconds_of_day % 60) as u32;
    // 1970-01-01 is a Thursday (weekday 4 in nginx's Sunday-based week).
    let weekday = (days + 4).rem_euclid(7) as u32;
    let (year, month, day) = civil_from_days(days);
    (year, month, day, hour, minute, second, weekday)
}

/// Howard Hinnant's `civil_from_days` algorithm (proleptic Gregorian).
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

/// Process-local UTC offset in seconds (nginx `tp->gmtoff * 60`).
fn local_offset_seconds() -> i64 {
    match time::UtcOffset::current_local_offset() {
        Ok(offset) => i64::from(offset.whole_seconds()),
        Err(_) => 0,
    }
}

/// nginx `ngx_escape_html`: `<`, `>`, `&`, `"` only.
fn html_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

/// nginx `ngx_escape_uri` with `NGX_ESCAPE_URI_COMPONENT`: every byte except
/// ALPHA / DIGIT / `-` / `.` / `_` / `~` becomes an uppercase `%XX`.
fn uri_escape(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0xf) as usize] as char);
        }
    }
    out
}

/// nginx `ngx_escape_json`.
fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            0x08 => out.push_str("\\b"),
            0x09 => out.push_str("\\t"),
            0x0a => out.push_str("\\n"),
            0x0c => out.push_str("\\f"),
            0x0d => out.push_str("\\r"),
            byte if byte <= 0x1f => {
                out.push_str("\\u00");
                out.push((b'0' + (byte >> 4)) as char);
                let low = byte & 0xf;
                out.push(if low < 10 {
                    (b'0' + low) as char
                } else {
                    (b'A' + low - 10) as char
                });
            }
            byte => out.push(byte as char),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool, is_file: bool, size: u64, mtime: i64) -> AutoindexEntry {
        AutoindexEntry {
            name: name.to_owned(),
            is_dir,
            is_file,
            size,
            mtime,
        }
    }

    fn policy(format: AutoindexFormat) -> AutoindexConfig {
        AutoindexConfig {
            enabled: true,
            exact_size: true,
            localtime: false,
            format,
        }
    }

    #[test]
    fn html_matches_nginx_shape() {
        let mut entries = vec![
            entry("b.txt", false, true, 5, 1_577_836_800),
            entry("adir", true, false, 0, 1_577_836_800),
        ];
        let rendered = render(&policy(AutoindexFormat::Html), "/list/", &mut entries, None);
        let body = String::from_utf8(rendered.body).expect("utf8");
        assert_eq!(rendered.content_type, "text/html");
        assert!(body.starts_with(
            "<html>\r\n<head><title>Index of /list/</title></head>\r\n<body>\r\n<h1>Index of /list/</h1><hr><pre><a href=\"../\">../</a>\r\n"
        ));
        // Directories sort first, dirs carry a trailing slash and a `-` size.
        let dir_line = body.lines().nth(4).unwrap_or_else(|| panic!("{body:?}"));
        assert!(
            dir_line.starts_with("<a href=\"adir/\">adir/"),
            "{dir_line}"
        );
        assert!(dir_line.contains(" 01-Jan-2020 00:00 "), "{dir_line}");
        let file_line = body.lines().nth(5).unwrap_or_else(|| panic!("{body:?}"));
        assert!(
            file_line.starts_with("<a href=\"b.txt\">b.txt"),
            "{file_line}"
        );
        assert!(file_line.contains(" 01-Jan-2020 00:00 "), "{file_line}");
        assert!(body.ends_with("</pre><hr></body>\r\n</html>\r\n"));
    }

    #[test]
    fn html_escapes_uri_href_and_display_name() {
        let mut entries = vec![entry("a b<&>.txt", false, true, 1, 0)];
        let body = String::from_utf8(
            render(&policy(AutoindexFormat::Html), "/list/", &mut entries, None).body,
        )
        .expect("utf8");
        assert!(body.contains("href=\"a%20b%3C%26%3E.txt\""), "{body}");
        assert!(body.contains(">a b&lt;&amp;&gt;.txt"), "{body}");
    }

    #[test]
    fn json_and_jsonp_match_nginx_shape() {
        let mut entries = vec![
            entry("adir", true, false, 0, 0),
            entry("a.txt", false, true, 12, 0),
        ];
        let json =
            String::from_utf8(render(&policy(AutoindexFormat::Json), "/", &mut entries, None).body)
                .expect("utf8");
        assert!(json.starts_with("[\r\n{ \"name\":\"adir\", \"type\":\"directory\", \"mtime\":\"Thu, 01 Jan 1970 00:00:00 GMT\" },"), "{json:?}");
        assert!(json.contains("\r\n{ \"name\":\"a.txt\", \"type\":\"file\", \"mtime\":\"Thu, 01 Jan 1970 00:00:00 GMT\", \"size\":12 }"), "{json}");
        assert!(json.ends_with("\r\n]"), "{json}");
        let jsonp = String::from_utf8(
            render(
                &policy(AutoindexFormat::Jsonp),
                "/",
                &mut entries,
                Some("cb"),
            )
            .body,
        )
        .expect("utf8");
        assert!(jsonp.starts_with("/* callback */\r\ncb(["), "{jsonp}");
        assert!(jsonp.ends_with(");"), "{jsonp}");
        // An empty listing still renders the array delimiters.
        let empty = String::from_utf8(
            render(&policy(AutoindexFormat::Json), "/", &mut Vec::new(), None).body,
        )
        .expect("utf8");
        assert_eq!(empty, "[\r\n]");
    }

    #[test]
    fn xml_matches_nginx_shape_and_publishes_utf8() {
        let mut entries = vec![
            entry("adir", true, false, 0, 0),
            entry("a.txt", false, true, 12, 0),
        ];
        let rendered = render(&policy(AutoindexFormat::Xml), "/", &mut entries, None);
        assert_eq!(rendered.content_type, "text/xml; charset=utf-8");
        let body = String::from_utf8(rendered.body).expect("utf8");
        assert_eq!(
            body,
            "<?xml version=\"1.0\"?>\r\n<list>\r\n<directory mtime=\"1970-01-01T00:00:00Z\">adir</directory>\r\n<file mtime=\"1970-01-01T00:00:00Z\" size=\"12\">a.txt</file>\r\n</list>\r\n"
        );
    }

    #[test]
    fn exact_size_off_renders_human_sizes() {
        let mut entries = vec![entry("k.txt", false, true, 10_000, 0)];
        let mut off = policy(AutoindexFormat::Html);
        off.exact_size = false;
        let body = String::from_utf8(render(&off, "/", &mut entries, None).body).expect("utf8");
        // 10000 bytes rounds to 10K with nginx's `%6iK` field.
        assert!(body.contains(" 01-Jan-1970 00:00     10K"), "{body}");
    }

    #[test]
    fn jsonp_callback_is_validated_like_nginx() {
        assert_eq!(validate_jsonp_callback("cb"), Some("cb"));
        assert_eq!(validate_jsonp_callback("a.b_1"), Some("a.b_1"));
        assert_eq!(validate_jsonp_callback(""), None);
        assert_eq!(validate_jsonp_callback("cb()"), None);
        assert_eq!(validate_jsonp_callback("cb;alert(1)"), None);
    }

    #[test]
    fn long_names_are_truncated_with_the_nginx_marker() {
        let mut entries = vec![entry(
            "a-very-long-file-name-that-exceeds-the-fifty-character-column.txt",
            false,
            true,
            1,
            0,
        )];
        let body =
            String::from_utf8(render(&policy(AutoindexFormat::Html), "/", &mut entries, None).body)
                .expect("utf8");
        assert!(body.contains("..&gt;</a>"), "{body}");
    }
}
