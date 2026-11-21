use super::remote::normalize_absolute;

pub struct CwdState {
    path: Option<String>,
    home: Option<String>,
    previous: Option<String>,
    pending_relative: Option<String>,
    editor: LineEditor,
    osc: OscParser,
    hide: EchoHide,
}

impl CwdState {
    pub fn new() -> Self {
        Self {
            path: None,
            home: None,
            previous: None,
            pending_relative: None,
            editor: LineEditor::default(),
            osc: OscParser::default(),
            hide: EchoHide::default(),
        }
    }

    pub fn path(&self) -> Option<String> {
        self.path.clone()
    }

    pub fn set_absolute_start(&mut self, path: &str) -> Option<String> {
        let path = normalize_absolute(path)?;
        self.set_path(path)
    }

    pub fn set_relative_start(&mut self, path: &str) {
        if !path.is_empty() && !path.contains('\0') && path.len() <= 512 {
            self.pending_relative = Some(path.to_string());
        }
    }

    pub fn set_home(&mut self, home: String) -> Option<String> {
        let home = normalize_absolute(&home)?;
        self.home = Some(home.clone());
        if self.path.is_some() {
            return None;
        }
        let path = match self.pending_relative.take() {
            Some(relative) => normalize_absolute(&format!(
                "{}/{}",
                home.trim_end_matches('/'),
                relative.trim_start_matches('/')
            ))
            .unwrap_or(home),
            None => home,
        };
        self.set_path(path)
    }

    pub fn feed_output(&mut self, data: &[u8]) -> Option<String> {
        let path = self.osc.push(data)?;
        self.set_path(path)
    }

    pub fn filter_output(&mut self, data: &[u8]) -> Vec<u8> {
        self.hide.apply(data)
    }

    pub fn arm_hide(&mut self, needle: &str) {
        self.hide.arm(needle);
    }

    pub fn disarm_hide(&mut self) {
        self.hide.disarm();
    }

    pub fn feed_input(&mut self, data: &[u8]) -> Option<String> {
        let submitted = self.editor.push(data)?;
        if submitted.unknown {
            return None;
        }
        let next = interpret_cd(
            &submitted.line,
            self.path.as_deref(),
            self.home.as_deref(),
            self.previous.as_deref(),
        )?;
        self.set_path(next)
    }

    fn set_path(&mut self, path: String) -> Option<String> {
        if self.path.as_deref() == Some(path.as_str()) {
            return None;
        }
        self.previous = self.path.clone();
        self.path = Some(path.clone());
        Some(path)
    }
}

struct Submitted {
    line: String,
    unknown: bool,
}

#[derive(Default)]
struct LineEditor {
    line: String,
    partial: Vec<u8>,
    unknown: bool,
}

impl LineEditor {
    fn push(&mut self, data: &[u8]) -> Option<Submitted> {
        for byte in data {
            if !self.partial.is_empty() || *byte >= 0x80 {
                self.partial.push(*byte);
                if let Ok(text) = std::str::from_utf8(&self.partial) {
                    if !self.unknown {
                        self.line.push_str(text);
                    }
                    self.partial.clear();
                } else if self.partial.len() >= 4 {
                    self.unknown = true;
                    self.partial.clear();
                }
                continue;
            }
            match *byte {
                b'\r' | b'\n' => {
                    let submitted = Submitted {
                        line: std::mem::take(&mut self.line),
                        unknown: self.unknown,
                    };
                    self.unknown = false;
                    self.partial.clear();
                    return Some(submitted);
                }
                0x03 => {
                    self.line.clear();
                    self.unknown = false;
                    self.partial.clear();
                }
                0x15 => self.line.clear(),
                0x17 => {
                    if !self.unknown {
                        let trimmed = self.line.trim_end();
                        self.line = match trimmed.rfind(char::is_whitespace) {
                            Some(index) => trimmed[..=index].trim_end().to_string(),
                            None => String::new(),
                        };
                    }
                }
                0x08 | 0x7f => {
                    if !self.unknown {
                        self.line.pop();
                    }
                }
                0x09 | 0x1b => self.unknown = true,
                byte if byte < 0x20 => self.unknown = true,
                byte => {
                    if !self.unknown {
                        self.line.push(byte as char);
                    }
                }
            }
        }
        None
    }
}

fn interpret_cd(
    line: &str,
    current: Option<&str>,
    home: Option<&str>,
    previous: Option<&str>,
) -> Option<String> {
    let line = line.trim();
    let rest = line.strip_prefix("cd")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let arg = rest.trim();
    if arg.is_empty() {
        return home.map(str::to_string);
    }
    if arg == "-" {
        return previous.map(str::to_string);
    }
    let arg = unquote(arg)?;
    if arg.contains([
        '$', '`', '*', '?', ';', '&', '|', '<', '>', '\\', '\n', '\r',
    ]) {
        return None;
    }
    if arg == "~" {
        return home.map(str::to_string);
    }
    if let Some(rest) = arg.strip_prefix("~/") {
        let home = home?;
        return normalize_absolute(&format!("{}/{}", home.trim_end_matches('/'), rest));
    }
    if arg.starts_with('~') {
        return None;
    }
    let current = current.unwrap_or("/");
    let combined = if arg.starts_with('/') {
        arg
    } else {
        format!("{}/{arg}", current.trim_end_matches('/'))
    };
    normalize_absolute(&combined)
}

fn unquote(arg: &str) -> Option<String> {
    if arg.is_empty() {
        return None;
    }
    let bytes = arg.as_bytes();
    let quote = bytes[0];
    if (quote == b'\'' || quote == b'"') && bytes.len() >= 2 && bytes[bytes.len() - 1] == quote {
        let inner = &arg[1..arg.len() - 1];
        if inner.contains(quote as char) {
            return None;
        }
        return Some(inner.to_string());
    }
    if arg.split_whitespace().count() != 1 {
        return None;
    }
    Some(arg.to_string())
}

#[derive(Default)]
struct OscParser {
    pending: Vec<u8>,
}

enum Take {
    NeedMore,
    Skip,
    Path(String),
}

impl OscParser {
    fn push(&mut self, data: &[u8]) -> Option<String> {
        self.pending.extend_from_slice(data);
        let mut found = None;
        loop {
            match self.take_one() {
                Take::NeedMore => break,
                Take::Skip => continue,
                Take::Path(path) => found = Some(path),
            }
        }
        if self.pending.len() > 8192 {
            let keep = self.pending.len() - 64;
            self.pending.drain(0..keep);
        }
        found
    }

    fn take_one(&mut self) -> Take {
        let Some(start) = self.pending.iter().position(|byte| *byte == 0x1b) else {
            self.pending.clear();
            return Take::NeedMore;
        };
        if start > 0 {
            self.pending.drain(0..start);
        }
        match header(&self.pending) {
            Header::Need => Take::NeedMore,
            Header::Reject => {
                self.pending.drain(0..1);
                Take::Skip
            }
            Header::Payload(payload_at) => {
                let tail = &self.pending[payload_at..];
                let bel = tail.iter().position(|byte| *byte == 0x07);
                let st = tail.windows(2).position(|pair| pair == [0x1b, b'\\']);
                let Some(end) = bel
                    .map(|index| (payload_at + index, 1))
                    .or_else(|| st.map(|index| (payload_at + index, 2)))
                else {
                    if self.pending.len() > 4096 {
                        self.pending.drain(0..1);
                        return Take::Skip;
                    }
                    return Take::NeedMore;
                };
                let text = String::from_utf8_lossy(&self.pending[payload_at..end.0]).into_owned();
                self.pending.drain(0..end.0 + end.1);
                let path = if payload_at == 4 {
                    path_from_osc7(&text)
                } else {
                    path_from_iterm(&text)
                };
                match path {
                    Some(path) => Take::Path(path),
                    None => Take::Skip,
                }
            }
        }
    }
}

enum Header {
    Need,
    Reject,
    Payload(usize),
}

fn header(pending: &[u8]) -> Header {
    if pending.len() < 2 {
        return Header::Need;
    }
    if pending[1] != b']' {
        return Header::Reject;
    }
    if pending.len() < 3 {
        return Header::Need;
    }
    if pending[2] == b'7' {
        if pending.len() < 4 {
            return Header::Need;
        }
        if pending[3] == b';' {
            return Header::Payload(4);
        }
        return Header::Reject;
    }
    const MARKER: &[u8] = b"1337;CurrentDir=";
    let rest = &pending[2..];
    if rest.starts_with(MARKER) {
        return Header::Payload(2 + MARKER.len());
    }
    if MARKER.starts_with(rest) {
        return Header::Need;
    }
    Header::Reject
}

fn path_from_osc7(payload: &str) -> Option<String> {
    let rest = payload.trim().strip_prefix("file://")?;
    let slash = rest.find('/')?;
    let decoded = percent_decode(&rest[slash..])?;
    normalize_absolute(&decoded)
}

fn path_from_iterm(payload: &str) -> Option<String> {
    let payload = payload.trim();
    if payload.starts_with("file://") {
        return path_from_osc7(payload);
    }
    let decoded = percent_decode(payload)?;
    normalize_absolute(&decoded)
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let high = from_hex(bytes[index + 1])?;
            let low = from_hex(bytes[index + 2])?;
            out.push((high << 4) | low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Default)]
struct EchoHide {
    needle: Vec<u8>,
    buffer: Vec<u8>,
    active: bool,
    shown: usize,
}

impl EchoHide {
    fn arm(&mut self, needle: &str) {
        self.needle = needle.as_bytes().to_vec();
        self.buffer.clear();
        self.shown = 0;
        self.active = !self.needle.is_empty();
    }

    fn disarm(&mut self) {
        self.active = false;
        self.buffer.clear();
    }

    fn apply(&mut self, data: &[u8]) -> Vec<u8> {
        if !self.active {
            return data.to_vec();
        }
        self.buffer.extend_from_slice(data);
        let mut visible = Vec::new();
        while self.active {
            let Some(offset) = self
                .buffer
                .iter()
                .position(|byte| *byte == b'\n' || *byte == b'\r')
            else {
                break;
            };
            let mut end = offset + 1;
            if self.buffer.get(offset) == Some(&b'\r') && self.buffer.get(end) == Some(&b'\n') {
                end += 1;
            }
            let line: Vec<u8> = self.buffer.drain(..end).collect();
            if find_slice(&line, &self.needle).is_some() {
                self.active = false;
                visible.extend_from_slice(&self.buffer);
                self.buffer.clear();
                return visible;
            }
            visible.extend_from_slice(&line);
        }
        if self.active && (self.buffer.len() > 4096 || !pending_hook_line(&self.buffer)) {
            visible.extend_from_slice(&self.buffer);
            self.buffer.clear();
        }
        self.shown += visible.len();
        if self.active && self.shown > 16 * 1024 {
            self.active = false;
            visible.extend_from_slice(&self.buffer);
            self.buffer.clear();
        }
        visible
    }
}

fn pending_hook_line(buffer: &[u8]) -> bool {
    if find_slice(buffer, b"printf").is_some() {
        return true;
    }
    let marker = b"printf";
    let max = buffer.len().min(marker.len() - 1);
    (1..=max).any(|len| buffer[buffer.len() - len..] == marker[..len])
}

fn find_slice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::CwdState;

    #[test]
    fn follows_osc_7_across_chunks() {
        let mut cwd = CwdState::new();
        assert!(
            cwd.feed_output(b"\x1b]7;file://host/opt/my%20dir")
                .is_none()
        );
        assert_eq!(cwd.feed_output(b"\x07$ ").as_deref(), Some("/opt/my dir"));
        assert_eq!(
            cwd.feed_output(b"\x1b]1337;CurrentDir=/var/log\x07")
                .as_deref(),
            Some("/var/log")
        );
    }

    #[test]
    fn tracks_plain_cd_and_hides_the_hook_echo() {
        let mut cwd = CwdState::new();
        assert_eq!(cwd.set_home("/root".to_string()).as_deref(), Some("/root"));
        assert!(cwd.feed_input(b"cd /opt/zd").is_none());
        assert_eq!(cwd.feed_input(b"x\r").as_deref(), Some("/opt/zdx"));
        assert_eq!(cwd.feed_input(b"cd\r").as_deref(), Some("/root"));
        assert_eq!(cwd.feed_input(b"cd -\r").as_deref(), Some("/opt/zdx"));
        assert!(cwd.feed_input(b"cd /opt && ls\r").is_none());
        cwd.arm_hide("PIXOabcd");
        assert_eq!(
            cwd.filter_output(b"Welcome\nroot# ").as_slice(),
            b"Welcome\nroot# "
        );
        assert!(cwd.filter_output(b"printf cwd # PIXOabcd").is_empty());
        assert_eq!(cwd.filter_output(b"\nready").as_slice(), b"ready");
        cwd.arm_hide("PIXOabcd");
        assert_eq!(
            cwd.filter_output(b"root# printf # PIXOabcd\r\nroot# ")
                .as_slice(),
            b"root# "
        );
    }
}
