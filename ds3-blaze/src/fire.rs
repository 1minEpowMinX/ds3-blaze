//! Blaze 3 ("Fire") frame codec.
//!
//! Frame header, big endian:
//! `length:u16 component:u16 command:u16 error:u16 type<<4:u8 options<<4:u8 seq:u16`,
//! followed by `length_hi:u16` when [`OPT_JUMBO_FRAME`] is set.

use crate::components;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt;
use std::io;
use tdf::{TdfDeserializer, TdfSerialize, TdfStringifier, serialize_vec};
use tokio_util::codec::{Decoder, Encoder};

/// Option bit that extends the frame length to 32 bits.
pub const OPT_JUMBO_FRAME: u8 = 0x1;

const HEADER_SIZE: usize = 12;
const JUMBO_SIZE: usize = 2;

/// Frame direction and kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Request,
    Response,
    Notify,
    Error,
    /// A value outside the four known kinds, kept verbatim for logging.
    Other(u8),
}

impl FrameType {
    fn from_bits(value: u8) -> Self {
        match value {
            0 => Self::Request,
            1 => Self::Response,
            2 => Self::Notify,
            3 => Self::Error,
            other => Self::Other(other),
        }
    }

    fn bits(self) -> u8 {
        match self {
            Self::Request => 0,
            Self::Response => 1,
            Self::Notify => 2,
            Self::Error => 3,
            Self::Other(value) => value,
        }
    }
}

/// Decoded frame header.
#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub component: u16,
    pub command: u16,
    pub error: u16,
    pub ty: FrameType,
    pub options: u8,
    pub seq: u16,
}

/// A frame header with its TDF-encoded body.
pub struct Frame {
    pub header: FrameHeader,
    pub contents: Bytes,
}

impl Frame {
    /// Returns a request frame carrying `value` as its body.
    pub fn request<V: TdfSerialize>(seq: u16, component: u16, command: u16, value: V) -> Frame {
        Frame {
            header: FrameHeader {
                component,
                command,
                error: 0,
                ty: FrameType::Request,
                options: 0,
                seq,
            },
            contents: Bytes::from(serialize_vec(&value)),
        }
    }

    /// Returns a response to `request` carrying `value` as its body.
    pub fn response<V: TdfSerialize>(request: &FrameHeader, value: V) -> Frame {
        Self::response_raw(request, Bytes::from(serialize_vec(&value)))
    }

    /// Returns a response to `request` with an empty body.
    pub fn response_empty(request: &FrameHeader) -> Frame {
        Self::response_raw(request, Bytes::new())
    }

    /// Returns an error reply to `request` with error code `code` and an empty body.
    pub fn error(request: &FrameHeader, code: u16) -> Frame {
        let mut frame = Self::response_raw(request, Bytes::new());
        frame.header.ty = FrameType::Error;
        frame.header.error = code;
        frame
    }

    /// Returns a notification frame carrying `value` as its body.
    pub fn notify<V: TdfSerialize>(component: u16, command: u16, value: V) -> Frame {
        Frame {
            header: FrameHeader {
                component,
                command,
                error: 0,
                ty: FrameType::Notify,
                options: 0,
                seq: 0,
            },
            contents: Bytes::from(serialize_vec(&value)),
        }
    }

    fn response_raw(request: &FrameHeader, contents: Bytes) -> Frame {
        Frame {
            header: FrameHeader {
                component: request.component,
                command: request.command,
                error: 0,
                ty: FrameType::Response,
                options: 0,
                seq: request.seq,
            },
            contents,
        }
    }
}

/// Commands whose request bodies carry credentials and are left out of logs.
const HIDDEN_BODIES: &[(u16, u16)] = &[
    (
        components::authentication::COMPONENT,
        components::authentication::ORIGIN_LOGIN,
    ),
    (
        components::authentication::COMPONENT,
        components::authentication::SILENT_LOGIN,
    ),
    (
        components::authentication::COMPONENT,
        components::authentication::LOGIN,
    ),
];

impl fmt::Display for Frame {
    /// Formats the header on one line, then the body as a TDF tree, or as hex
    /// when the body does not parse as TDF. Request bodies of [`HIDDEN_BODIES`]
    /// commands are replaced by a placeholder.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let h = &self.header;
        let notify = h.ty == FrameType::Notify;
        writeln!(
            f,
            "{:?} seq={} {}->{} ({:#06x}->{:#06x}) error={:#06x} options={:#x} len={}",
            h.ty,
            h.seq,
            components::component_name(h.component).unwrap_or("?"),
            components::command_name(h.component, h.command, notify).unwrap_or("?"),
            h.component,
            h.command,
            h.error,
            h.options,
            self.contents.len()
        )?;
        if self.contents.is_empty() {
            return Ok(());
        }
        if h.ty == FrameType::Request && HIDDEN_BODIES.contains(&(h.component, h.command)) {
            return writeln!(f, "(body hidden: credentials)");
        }
        let (tree, complete) =
            TdfStringifier::<String>::new_string(TdfDeserializer::new(&self.contents));
        f.write_str(&tree)?;
        if !complete {
            writeln!(f, "\n(TDF parse stopped) raw: {}", hex(&self.contents))?;
        }
        Ok(())
    }
}

/// Returns `data` as lowercase hex with a space between bytes.
pub fn hex(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Stateful decoder and encoder for [`Frame`].
#[derive(Default)]
pub struct FireCodec {
    pending: Option<(FrameHeader, usize, bool)>,
}

impl Decoder for FireCodec {
    type Item = Frame;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame>, io::Error> {
        if self.pending.is_none() {
            if src.len() < HEADER_SIZE {
                return Ok(None);
            }
            let length = src.get_u16() as usize;
            let header = FrameHeader {
                component: src.get_u16(),
                command: src.get_u16(),
                error: src.get_u16(),
                ty: FrameType::from_bits(src.get_u8() >> 4),
                options: src.get_u8() >> 4,
                seq: src.get_u16(),
            };
            let jumbo = header.options & OPT_JUMBO_FRAME != 0;
            self.pending = Some((header, length, jumbo));
        }

        let (_, length, jumbo) = self.pending.as_mut().expect("pending header");
        if *jumbo {
            if src.len() < JUMBO_SIZE {
                return Ok(None);
            }
            *length |= (src.get_u16() as usize) << 16;
            *jumbo = false;
        }
        if src.len() < *length {
            return Ok(None);
        }

        let (header, length, _) = self.pending.take().expect("pending header");
        let contents = src.split_to(length).freeze();
        Ok(Some(Frame { header, contents }))
    }
}

impl Encoder<Frame> for FireCodec {
    type Error = io::Error;

    fn encode(&mut self, frame: Frame, dst: &mut BytesMut) -> Result<(), io::Error> {
        let h = &frame.header;
        let length = frame.contents.len();
        let mut options = h.options;
        if length > u16::MAX as usize {
            options |= OPT_JUMBO_FRAME;
        }
        dst.put_u16(length as u16);
        dst.put_u16(h.component);
        dst.put_u16(h.command);
        dst.put_u16(h.error);
        dst.put_u8(h.ty.bits() << 4);
        dst.put_u8(options << 4);
        dst.put_u16(h.seq);
        if options & OPT_JUMBO_FRAME != 0 {
            dst.put_u16((length >> 16) as u16);
        }
        dst.extend_from_slice(&frame.contents);
        Ok(())
    }
}
