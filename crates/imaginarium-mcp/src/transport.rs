//! Newline-delimited JSON-RPC over stdio (Cerebro/ApexOS MCP wire format).

use anyhow::Result;
use serde_json::Value;
use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

const MAX_FRAME_BYTES: u64 = 16 * 1024 * 1024;

pub enum Frame {
    Value(Value),
    Eof,
    ParseError(String),
    Oversized { bytes: u64 },
}

/// Writer half — cloneable so `tools/call` can finish after the read loop
/// has already accepted a ping.
#[derive(Clone)]
pub struct SharedWriter {
    inner: std::sync::Arc<tokio::sync::Mutex<tokio::io::Stdout>>,
}

impl SharedWriter {
    pub fn new(writer: tokio::io::Stdout) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(writer)),
        }
    }

    pub async fn write(&self, value: &Value) -> Result<()> {
        let mut buf = serde_json::to_string(value)?;
        buf.push('\n');
        let mut w = self.inner.lock().await;
        w.write_all(buf.as_bytes()).await?;
        w.flush().await?;
        Ok(())
    }
}

pub struct StdioTransport {
    reader: BufReader<tokio::io::Stdin>,
    writer: SharedWriter,
}

impl StdioTransport {
    pub fn new() -> Self {
        Self {
            reader: BufReader::new(stdin()),
            writer: SharedWriter::new(stdout()),
        }
    }

    pub fn writer(&self) -> SharedWriter {
        self.writer.clone()
    }

    pub async fn read(&mut self) -> Result<Frame> {
        let mut line = String::new();
        // Cap one frame to avoid unbounded buffer growth.
        let n = {
            let mut limited = (&mut self.reader).take(MAX_FRAME_BYTES);
            limited.read_line(&mut line).await? as u64
        };
        if n == 0 {
            return Ok(Frame::Eof);
        }
        if n >= MAX_FRAME_BYTES && !line.ends_with('\n') {
            loop {
                let buf = self.reader.fill_buf().await?;
                if buf.is_empty() {
                    break;
                }
                if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                    self.reader.consume(pos + 1);
                    break;
                }
                let len = buf.len();
                self.reader.consume(len);
            }
            return Ok(Frame::Oversized { bytes: n });
        }
        match serde_json::from_str(line.trim()) {
            Ok(v) => Ok(Frame::Value(v)),
            Err(e) => Ok(Frame::ParseError(e.to_string())),
        }
    }

    pub async fn write(&mut self, value: &Value) -> Result<()> {
        self.writer.write(value).await
    }
}
