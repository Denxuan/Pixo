use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub icon: String,
}

/// Reads an application desktop file. `locale` is a language tag such as `zh_CN`.
pub fn parse_desktop(text: &str, locale: &str) -> Option<DesktopEntry> {
    let mut section = String::new();
    let mut values = HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            section = name.trim().to_string();
            continue;
        }
        if section != "Desktop Entry" {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        values.insert(key.trim().to_string(), unescape(value.trim()));
    }
    if values
        .get("Type")
        .is_some_and(|kind| !kind.eq_ignore_ascii_case("Application"))
    {
        return None;
    }
    if flag_true(values.get("NoDisplay")) || flag_true(values.get("Hidden")) {
        return None;
    }
    let name = localized(&values, "Name", locale)?;
    let exec = exec_program(values.get("Exec")?)?;
    let icon = values.get("Icon").cloned().unwrap_or_default();
    Some(DesktopEntry { name, exec, icon })
}

pub fn exec_program(exec: &str) -> Option<String> {
    let exec = exec.trim();
    if exec.is_empty() {
        return None;
    }
    let mut chars = exec.chars().peekable();
    let mut token = String::new();
    if chars.peek() == Some(&'"') {
        chars.next();
        for char in chars {
            if char == '"' {
                break;
            }
            token.push(char);
        }
    } else {
        for char in chars {
            if char.is_whitespace() {
                break;
            }
            token.push(char);
        }
    }
    let token = token.trim();
    if token.is_empty() || token.starts_with('%') {
        None
    } else {
        Some(token.to_string())
    }
}

fn localized(values: &HashMap<String, String>, key: &str, locale: &str) -> Option<String> {
    let locale = locale.split(['.', '@']).next().unwrap_or(locale);
    let language = locale.split('_').next().unwrap_or(locale);
    for candidate in [
        format!("{key}[{locale}]"),
        format!("{key}[{language}]"),
        key.to_string(),
    ] {
        if let Some(value) = values.get(&candidate).map(|value| value.trim()) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn flag_true(value: Option<&String>) -> bool {
    value.is_some_and(|value| {
        let value = value.trim();
        value.eq_ignore_ascii_case("true") || value == "1"
    })
}

fn unescape(value: &str) -> String {
    let mut text = String::new();
    let mut chars = value.chars();
    while let Some(char) = chars.next() {
        if char != '\\' {
            text.push(char);
            continue;
        }
        match chars.next() {
            Some('s') => text.push(' '),
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some('r') => text.push('\r'),
            Some('\\') => text.push('\\'),
            Some(other) => text.push(other),
            None => {}
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{exec_program, parse_desktop};

    #[test]
    fn reads_a_localized_application() {
        let entry = parse_desktop(
            "[Desktop Entry]\nType=Application\nName=Firefox\nName[zh_CN]=火狐\nExec=firefox %u\nIcon=firefox\n",
            "zh_CN.UTF-8",
        )
        .unwrap();
        assert_eq!(entry.name, "火狐");
        assert_eq!(entry.exec, "firefox");
        assert_eq!(entry.icon, "firefox");
    }

    #[test]
    fn skips_hidden_and_non_applications() {
        assert!(
            parse_desktop(
                "[Desktop Entry]\nType=Application\nNoDisplay=true\nName=Hidden\nExec=hidden\n",
                "en"
            )
            .is_none()
        );
        assert!(
            parse_desktop("[Desktop Entry]\nType=Link\nName=Site\nExec=site\n", "en").is_none()
        );
    }

    #[test]
    fn exec_keeps_a_quoted_path() {
        assert_eq!(
            exec_program("\"/usr/bin/my app\" %f"),
            Some("/usr/bin/my app".to_string())
        );
    }
}
