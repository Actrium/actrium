//! WebSocket subsystem
//!
//! WebSocket Connection implementation

pub(crate) mod connection;
pub(crate) mod gate;

pub(crate) use connection::WebSocketConnection;
pub(crate) use gate::WebSocketGate;
