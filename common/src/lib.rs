#![no_std]

// Simple message types for ESP-NOW communication
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum MessageType {
    Ping = 0x01,
    Pong = 0x02,
    Move = 0x03,
}

impl MessageType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(MessageType::Ping),
            0x02 => Some(MessageType::Pong),
            0x03 => Some(MessageType::Move),
            _ => None,
        }
    }
}

/// Movement command: [MessageType::Move, x_high, x_low, y_high, y_low]
/// x, y are signed i16 values from -100 to 100
#[derive(Clone, Copy, Debug, Default)]
pub struct MoveCommand {
    pub x: i16,  // -100 (left) to 100 (right)
    pub y: i16,  // -100 (back) to 100 (forward)
}

impl MoveCommand {
    pub fn to_bytes(&self) -> [u8; 5] {
        [
            MessageType::Move as u8,
            (self.x >> 8) as u8,
            self.x as u8,
            (self.y >> 8) as u8,
            self.y as u8,
        ]
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() >= 5 && data[0] == MessageType::Move as u8 {
            Some(MoveCommand {
                x: ((data[1] as i16) << 8) | (data[2] as i16),
                y: ((data[3] as i16) << 8) | (data[4] as i16),
            })
        } else {
            None
        }
    }
}
