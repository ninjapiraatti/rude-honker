#![no_std]

// Simple message types for ESP-NOW communication
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum MessageType {
    Ping = 0x01,
    Pong = 0x02,
}

impl MessageType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(MessageType::Ping),
            0x02 => Some(MessageType::Pong),
            _ => None,
        }
    }
}
