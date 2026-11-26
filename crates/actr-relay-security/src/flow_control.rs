//! Transport-independent receive credit accounting for a peer direction.

/// A reserved WebRTC frame. The zero fragment count cannot be an application frame.
pub const RTC_CONTROL_PREFIX: &[u8; 8] = b"ACTR\0\0\0\0";
pub const RTC_CREDITED_FRAGMENT: u16 = 1 << 15;

/// Separate the reliable RPC window from the stream window so blocked stream
/// consumers cannot prevent control-plane-adjacent RPCs on the same peer.
#[derive(Clone, Copy, Debug)]
pub enum Class {
    Rpc = 0,
    Stream = 1,
}

impl Class {
    pub const ALL: [Self; 2] = [Self::Rpc, Self::Stream];

    pub fn from_payload_tag(tag: u8) -> Option<Self> {
        match tag {
            0 | 1 => Some(Self::Rpc),
            2 | 3 => Some(Self::Stream),
            _ => None,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// A budget expressed in both application bytes and complete messages.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Credit {
    pub bytes: u64,
    pub messages: u64,
}

impl Credit {
    pub fn message(bytes: usize) -> Self {
        Self {
            bytes: (bytes as u64).max(128),
            messages: 1,
        }
    }

    pub fn covers(self, other: Self) -> bool {
        self.bytes >= other.bytes && self.messages >= other.messages
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            bytes: self.bytes.checked_add(other.bytes)?,
            messages: self.messages.checked_add(other.messages)?,
        })
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        Some(Self {
            bytes: self.bytes.checked_sub(other.bytes)?,
            messages: self.messages.checked_sub(other.messages)?,
        })
    }
}

/// Cumulative grants are idempotent and must never shrink in a live direction.
#[derive(Clone, Copy, Default)]
pub struct Window {
    granted: Credit,
    used: Credit,
}

impl Window {
    pub fn update(&mut self, granted: Credit, limit: Credit) -> bool {
        if !granted.covers(self.granted)
            || !granted
                .checked_sub(self.used)
                .is_some_and(|available| limit.covers(available))
        {
            return false;
        }
        self.granted = granted;
        true
    }

    pub fn available(&self) -> Credit {
        self.granted
            .checked_sub(self.used)
            .expect("flow credit never goes negative")
    }

    pub fn consume(&mut self, credit: Credit) -> bool {
        if !self.available().covers(credit) {
            return false;
        }
        self.used = self.used.checked_add(credit).expect("bounded by grant");
        true
    }

    pub fn grant(&mut self, credit: Credit) -> Option<Credit> {
        self.granted = self.granted.checked_add(credit)?;
        Some(self.granted)
    }
}

/// Control records are scoped by the authenticated channel and its direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Control {
    Window([Credit; 2]),
    Request { id: u64, bytes: u32 },
    Reserved { id: u64 },
    Release { id: u64 },
}

impl Control {
    pub fn encode(self) -> Vec<u8> {
        let mut result = Vec::with_capacity(33);
        match self {
            Self::Window(credits) => {
                result.push(0);
                for credit in credits {
                    result.extend_from_slice(&credit.bytes.to_be_bytes());
                    result.extend_from_slice(&credit.messages.to_be_bytes());
                }
            }
            Self::Request { id, bytes } => {
                result.push(1);
                result.extend_from_slice(&id.to_be_bytes());
                result.extend_from_slice(&bytes.to_be_bytes());
            }
            Self::Reserved { id } => {
                result.push(2);
                result.extend_from_slice(&id.to_be_bytes());
            }
            Self::Release { id } => {
                result.push(3);
                result.extend_from_slice(&id.to_be_bytes());
            }
        }
        result
    }

    pub fn decode(input: &[u8]) -> Option<Self> {
        let (kind, rest) = input.split_first()?;
        let (first, tail) = rest.split_at_checked(8)?;
        let first = u64::from_be_bytes(first.try_into().ok()?);
        match (kind, tail.len()) {
            (0, 24) => Some(Self::Window([
                Credit {
                    bytes: first,
                    messages: u64::from_be_bytes(tail[..8].try_into().ok()?),
                },
                Credit {
                    bytes: u64::from_be_bytes(tail[8..16].try_into().ok()?),
                    messages: u64::from_be_bytes(tail[16..].try_into().ok()?),
                },
            ])),
            (1, 4) if first > 0 => Some(Self::Request {
                id: first,
                bytes: u32::from_be_bytes(tail.try_into().ok()?),
            }),
            (2, 0) if first > 0 => Some(Self::Reserved { id: first }),
            (3, 0) if first > 0 => Some(Self::Release { id: first }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cumulative_updates_do_not_duplicate_or_shrink_credit() {
        let limit = Credit {
            bytes: 512,
            messages: 2,
        };
        let mut window = Window::default();
        assert!(window.update(limit, limit));
        assert!(window.consume(Credit::message(10)));
        assert!(window.update(limit, limit));
        assert_eq!(
            window.available(),
            Credit {
                bytes: 384,
                messages: 1
            }
        );
        assert!(!window.update(
            Credit {
                bytes: 500,
                messages: 2
            },
            limit
        ));
        assert!(!window.update(
            Credit {
                bytes: 700,
                messages: 3
            },
            limit
        ));
        let update = limit.checked_add(Credit::message(10)).unwrap();
        assert!(window.update(update, limit));
        assert_eq!(window.available(), limit);
    }

    #[test]
    fn control_records_require_the_complete_shape() {
        for control in [
            Control::Window([Credit::message(123), Credit::message(456)]),
            Control::Request {
                id: 1,
                bytes: 32_000,
            },
            Control::Reserved { id: 2 },
            Control::Release { id: 3 },
        ] {
            let mut record = control.encode();
            assert_eq!(Control::decode(&record), Some(control));
            record.push(0);
            assert_eq!(Control::decode(&record), None);
        }
    }
}
