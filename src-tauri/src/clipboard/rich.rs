use super::model::RichText;

pub fn normalize(mut text: RichText) -> RichText {
    text.plain = text.plain.trim_end_matches('\0').to_string();
    if text.plain.trim().is_empty() {
        if let Some(html) = text.html.as_deref() {
            text.plain = plain_from_html(html);
        }
    }
    text.html = text
        .html
        .take()
        .map(|html| html.trim().to_string())
        .filter(|html| !html.is_empty());
    text.rtf = text.rtf.take().filter(|rtf| !rtf.is_empty());
    text
}

pub fn fit(mut text: RichText, max_bytes: u64) -> RichText {
    let max = usize::try_from(max_bytes).unwrap_or(usize::MAX);
    if text.html.as_ref().is_some_and(|html| html.len() > max) {
        text.html = None;
    }
    if text.rtf.as_ref().is_some_and(|rtf| rtf.len() > max) {
        text.rtf = None;
    }
    text
}

pub fn plain_from_html(html: &str) -> String {
    let mut plain = String::new();
    let mut inside_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                if !plain.ends_with(' ') {
                    plain.push(' ');
                }
            }
            _ if !inside_tag => plain.push(ch),
            _ => {}
        }
    }
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn html_from_windows(bytes: &[u8]) -> Option<String> {
    let raw = String::from_utf8_lossy(bytes);
    let html = if raw.contains("StartHTML:") {
        marked_slice(&raw, "StartFragment:", "EndFragment:")
            .or_else(|| marked_slice(&raw, "StartHTML:", "EndHTML:"))
            .unwrap_or_default()
    } else {
        raw.trim().to_string()
    };
    let html = html.trim().to_string();
    (!html.is_empty()).then_some(html)
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn windows_html(html: &str) -> Vec<u8> {
    if html.contains("StartHTML:") {
        return html.as_bytes().to_vec();
    }
    let body = if html.contains("<!--StartFragment-->") && html.contains("<!--EndFragment-->") {
        html.to_string()
    } else {
        format!("<html><body><!--StartFragment-->{html}<!--EndFragment--></body></html>")
    };
    let header = "Version:0.9\r\nStartHTML:00000000\r\nEndHTML:00000000\r\nStartFragment:00000000\r\nEndFragment:00000000\r\n";
    let start_html = header.len();
    let start_fragment = start_html
        + body
            .find("<!--StartFragment-->")
            .map(|index| index + "<!--StartFragment-->".len())
            .unwrap_or(start_html);
    let end_fragment = start_html + body.find("<!--EndFragment-->").unwrap_or(body.len());
    let end_html = start_html + body.len();
    format!(
        "Version:0.9\r\nStartHTML:{start_html:08}\r\nEndHTML:{end_html:08}\r\nStartFragment:{start_fragment:08}\r\nEndFragment:{end_fragment:08}\r\n{body}"
    )
    .into_bytes()
}

fn marked_slice(raw: &str, start_key: &str, end_key: &str) -> Option<String> {
    let start = header_offset(raw, start_key)?;
    let end = header_offset(raw, end_key)?;
    if end < start || end > raw.len() {
        return None;
    }
    Some(raw[start..end].to_string())
}

fn header_offset(raw: &str, key: &str) -> Option<usize> {
    let line = raw.lines().find(|line| line.starts_with(key))?;
    line[key.len()..].trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{fit, html_from_windows, normalize, plain_from_html, windows_html};
    use crate::clipboard::model::RichText;

    #[test]
    fn strips_tags_when_plain_text_is_missing() {
        let text = normalize(RichText {
            plain: String::new(),
            html: Some("<p>Hello <b>there</b></p>".into()),
            rtf: None,
        });
        assert_eq!(text.plain, "Hello there");
        assert!(text.formatted());
    }

    #[test]
    fn drops_rich_formats_over_the_text_limit() {
        let text = fit(
            RichText {
                plain: "ok".into(),
                html: Some("x".repeat(8)),
                rtf: Some(b"rtf".to_vec()),
            },
            4,
        );
        assert_eq!(text.plain, "ok");
        assert!(text.html.is_none());
        assert!(text.rtf.is_some());
    }

    #[test]
    fn windows_html_round_trips_the_fragment() {
        let wrapped = windows_html("<b>Hi</b>");
        let html = html_from_windows(&wrapped).unwrap();
        assert_eq!(html, "<b>Hi</b>");
        assert_eq!(plain_from_html(&html), "Hi");
    }
}
