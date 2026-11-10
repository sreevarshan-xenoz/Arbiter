//! Versioned agent <-> daemon protocol (virtio-serial, length-prefixed JSON).
//!
//! Windows is treated as less trusted than the host: every inbound message is
//! size-limited, schema-checked (serde) and range-validated before use.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

pub const PROTOCOL_VERSION: u32 = 1;
/// Hard cap on a single frame (bytes). Larger frames are rejected unread.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_PROCESSES: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("frame of {0} bytes exceeds limit of {MAX_FRAME_BYTES}")]
    FrameTooLarge(usize),
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("malformed json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid field: {0}")]
    Invalid(&'static str),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub name: String,
    pub cpu_percent: f32,
    pub mem_mb: u64,
    pub gpu_percent: f32,
}

/// What the Windows agent reports every 1-2 s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentReport {
    pub foreground: Option<String>,
    pub cpu_percent: f32,
    pub mem_used_mb: u64,
    pub gpu_heavy: bool,
    pub idle_secs: u64,
    pub processes: Vec<ProcessInfo>,
}

impl AgentReport {
    pub fn validate(&self) -> Result<(), ProtoError> {
        let pct = |v: f32| v.is_finite() && (0.0..=100.0).contains(&v);
        if !pct(self.cpu_percent) {
            return Err(ProtoError::Invalid("cpu_percent"));
        }
        if self.processes.len() > MAX_PROCESSES {
            return Err(ProtoError::Invalid("too many processes"));
        }
        if self.foreground.as_deref().is_some_and(|s| s.len() > 260) {
            return Err(ProtoError::Invalid("foreground too long"));
        }
        for p in &self.processes {
            if !pct(p.cpu_percent) || !pct(p.gpu_percent) || p.name.len() > 260 {
                return Err(ProtoError::Invalid("process entry"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    Hello {
        protocol: u32,
        agent_version: String,
    },
    Report(AgentReport),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostMessage {
    Welcome { protocol: u32 },
    Ack,
}

pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), ProtoError> {
    let body = serde_json::to_vec(msg)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(ProtoError::FrameTooLarge(body.len()));
    }
    w.write_all(&(body.len() as u32).to_be_bytes())?;
    w.write_all(&body)?;
    Ok(())
}

pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(r: &mut R) -> Result<T, ProtoError> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(ProtoError::FrameTooLarge(len));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(serde_json::from_slice(&body)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> AgentReport {
        AgentReport {
            foreground: Some("devenv.exe".into()),
            cpu_percent: 82.5,
            mem_used_mb: 6144,
            gpu_heavy: false,
            idle_secs: 0,
            processes: vec![ProcessInfo {
                name: "msbuild.exe".into(),
                cpu_percent: 70.0,
                mem_mb: 900,
                gpu_percent: 0.0,
            }],
        }
    }

    #[test]
    fn roundtrip() {
        let msg = AgentMessage::Report(report());
        let mut buf = Vec::new();
        write_frame(&mut buf, &msg).unwrap();
        let back: AgentMessage = read_frame(&mut buf.as_slice()).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn oversize_frame_rejected_without_reading_body() {
        let mut buf = ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes().to_vec();
        buf.extend_from_slice(b"{}");
        let r: Result<AgentMessage, _> = read_frame(&mut buf.as_slice());
        assert!(matches!(r, Err(ProtoError::FrameTooLarge(_))));
    }

    #[test]
    fn malformed_json_rejected() {
        let mut buf = 3u32.to_be_bytes().to_vec();
        buf.extend_from_slice(b"{{{");
        let r: Result<AgentMessage, _> = read_frame(&mut buf.as_slice());
        assert!(matches!(r, Err(ProtoError::Json(_))));
    }

    #[test]
    fn out_of_range_values_rejected() {
        let mut r = report();
        r.cpu_percent = 250.0;
        assert!(r.validate().is_err());
        r.cpu_percent = f32::NAN;
        assert!(r.validate().is_err());
        assert!(report().validate().is_ok());
    }
}
