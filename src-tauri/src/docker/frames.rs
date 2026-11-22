#![allow(dead_code)]

const MAX_FRAME: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogChunk {
    pub stream: LogStream,
    pub text: String,
}

/// Splits a Docker multiplexed log stream. Incomplete frames stay in `buffer`.
pub fn push_frames(buffer: &mut Vec<u8>, input: &[u8]) -> Vec<LogChunk> {
    buffer.extend_from_slice(input);
    let mut chunks = Vec::new();
    loop {
        if buffer.len() < 8 {
            break;
        }
        let size = u32::from_be_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]) as usize;
        if size > MAX_FRAME {
            buffer.clear();
            break;
        }
        if buffer.len() < 8 + size {
            break;
        }
        let kind = buffer[0];
        let payload: Vec<u8> = buffer.drain(..8 + size).skip(8).collect();
        let stream = match kind {
            2 => LogStream::Stderr,
            1 => LogStream::Stdout,
            _ => continue,
        };
        if payload.is_empty() {
            continue;
        }
        chunks.push(LogChunk {
            stream,
            text: String::from_utf8_lossy(&payload).into_owned(),
        });
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::{LogStream, push_frames};

    fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![kind, 0, 0, 0];
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn frames_split_stdout_and_stderr_across_chunks() {
        let stdout = frame(1, b"hello\n");
        let stderr = frame(2, b"warn");
        let mut buffer = Vec::new();
        let first = push_frames(&mut buffer, &stdout[..6]);
        assert!(first.is_empty());
        assert!(!buffer.is_empty());
        let mut rest = stdout[6..].to_vec();
        rest.extend_from_slice(&stderr);
        let chunks = push_frames(&mut buffer, &rest);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].stream, LogStream::Stdout);
        assert_eq!(chunks[0].text, "hello\n");
        assert_eq!(chunks[1].stream, LogStream::Stderr);
        assert_eq!(chunks[1].text, "warn");
        assert!(buffer.is_empty());
    }

    #[test]
    fn oversized_frame_is_dropped() {
        let mut buffer = Vec::new();
        let mut header = vec![1, 0, 0, 0];
        header.extend_from_slice(&((1024 * 1024) as u32).to_be_bytes());
        let chunks = push_frames(&mut buffer, &header);
        assert!(chunks.is_empty());
        assert!(buffer.is_empty());
    }
}
