//! Bounded kernel ALPC/LPC transport core for the native subsystem path.
//!
//! This is a kernel-internal service layer. It deliberately does not pretend
//! to be a complete Windows NT ALPC object manager: user handles, security
//! attributes, section views, completion lists, and blocking waits are layered
//! on top as separate syscall/object-manager work.
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const STATUS_SUCCESS: u32 = 0;
pub const STATUS_PENDING: u32 = 0x0000_0103;
pub const STATUS_INVALID_PARAMETER: u32 = 0xC000_000D;
pub const STATUS_INVALID_HANDLE: u32 = 0xC000_0008;
pub const STATUS_OBJECT_NAME_COLLISION: u32 = 0xC000_0035;
pub const STATUS_OBJECT_NAME_NOT_FOUND: u32 = 0xC000_0034;
pub const STATUS_PORT_DISCONNECTED: u32 = 0xC000_0037;
pub const STATUS_PORT_NOT_SET: u32 = 0xC000_0038;
pub const STATUS_BUFFER_TOO_SMALL: u32 = 0xC000_0023;

const MAX_PORTS: usize = 32;
const MAX_CONNECTIONS: usize = 64;
const MAX_MESSAGES: usize = 128;
const MAX_NAME: usize = 64;
pub const MAX_MESSAGE_SIZE: usize = 512;
const QUEUE_DEPTH: usize = 16;

#[derive(Clone, Copy)]
struct Port {
    active: bool,
    listening: bool,
    owner_pid: u64,
    name_len: u8,
    name: [u16; MAX_NAME],
    max_message: u16,
}
impl Port {
    const EMPTY: Self = Self { active: false, listening: false, owner_pid: 0, name_len: 0, name: [0; MAX_NAME], max_message: MAX_MESSAGE_SIZE as u16 };
}
#[derive(Clone, Copy)]
struct Connection {
    active: bool,
    accepted: bool,
    request_id: u32,
    server_port: u32,
    client_pid: u64,
    server_pid: u64,
    client_endpoint: u32,
    server_endpoint: u32,
}
impl Connection {
    const EMPTY: Self = Self { active: false, accepted: false, request_id: 0, server_port: 0, client_pid: 0, server_pid: 0, client_endpoint: 0, server_endpoint: 0 };
}
#[derive(Clone, Copy)]
struct Message {
    active: bool,
    source: u32,
    destination: u32,
    length: u16,
    sequence: u32,
    bytes: [u8; MAX_MESSAGE_SIZE],
}
impl Message {
    const EMPTY: Self = Self { active: false, source: 0, destination: 0, length: 0, sequence: 0, bytes: [0; MAX_MESSAGE_SIZE] };
}
struct State {
    ports: [Port; MAX_PORTS],
    connections: [Connection; MAX_CONNECTIONS],
    messages: [Message; MAX_MESSAGES],
    next_id: u32,
    next_message: u32,
}
impl State {
    const fn new() -> Self {
        Self { ports: [Port::EMPTY; MAX_PORTS], connections: [Connection::EMPTY; MAX_CONNECTIONS], messages: [Message::EMPTY; MAX_MESSAGES], next_id: 1, next_message: 1 }
    }
}
struct LockedState { lock: AtomicBool, state: core::cell::UnsafeCell<State> }
unsafe impl Sync for LockedState {}
static STATE: LockedState = LockedState { lock: AtomicBool::new(false), state: core::cell::UnsafeCell::new(State::new()) };
struct Guard;
impl Guard {
    fn acquire() -> Self {
        while STATE.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() { core::hint::spin_loop(); }
        Self
    }
    fn state(&mut self) -> &mut State { unsafe { &mut *STATE.state.get() } }
}
impl Drop for Guard { fn drop(&mut self) { STATE.lock.store(false, Ordering::Release); } }

fn alloc_id(state: &mut State) -> u32 {
    let id = state.next_id.max(1);
    state.next_id = id.wrapping_add(1).max(1);
    id
}
fn endpoint_connection(state: &State, endpoint: u32) -> Option<usize> {
    state.connections.iter().position(|c| c.active && (c.client_endpoint == endpoint || c.server_endpoint == endpoint))
}

/// Create a named server port. Name is UTF-16 without a trailing NUL.
pub fn create_port(name: &[u16], owner_pid: u64, max_message: usize) -> Result<u32, u32> {
    if name.is_empty() || name.len() >= MAX_NAME || owner_pid == 0 || max_message == 0 || max_message > MAX_MESSAGE_SIZE {
        return Err(STATUS_INVALID_PARAMETER);
    }
    let mut g = Guard::acquire();
    let s = g.state();
    if s.ports.iter().any(|p| p.active && p.name_len as usize == name.len() && p.name[..name.len()] == *name) {
        return Err(STATUS_OBJECT_NAME_COLLISION);
    }
    let slot = s.ports.iter().position(|p| !p.active).ok_or(STATUS_PENDING)?;
    let mut port = Port::EMPTY;
    port.active = true;
    port.owner_pid = owner_pid;
    port.name_len = name.len() as u8;
    port.name[..name.len()].copy_from_slice(name);
    port.max_message = max_message as u16;
    s.ports[slot] = port;
    Ok((slot as u32) + 1)
}

/// Mark a named port ready to accept connection requests.
pub fn listen(port_id: u32, owner_pid: u64) -> Result<(), u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let p = s.ports.get_mut(port_id.checked_sub(1).ok_or(STATUS_INVALID_HANDLE)? as usize).ok_or(STATUS_INVALID_HANDLE)?;
    if !p.active || p.owner_pid != owner_pid { return Err(STATUS_INVALID_HANDLE); }
    p.listening = true;
    Ok(())
}

/// Retrieve the oldest unaccepted connection request for a listening port.
/// A caller may retry after STATUS_PENDING; a scheduler wait queue can replace
/// polling without changing this request/accept contract.
pub fn listen_next(port_id: u32, owner_pid: u64) -> Result<u32, u32> {
    let g = Guard::acquire();
    let s = unsafe { &*STATE.state.get() };
    let index = port_id.checked_sub(1).ok_or(STATUS_INVALID_HANDLE)? as usize;
    let port = s.ports.get(index).ok_or(STATUS_INVALID_HANDLE)?;
    if !port.active || !port.listening || port.owner_pid != owner_pid {
        return Err(STATUS_INVALID_HANDLE);
    }
    s.connections.iter()
        .filter(|c| c.active && !c.accepted && c.server_port == port_id)
        .min_by_key(|c| c.request_id)
        .map(|c| c.request_id)
        .ok_or(STATUS_PENDING)
}

/// Queue a client connection request. The returned request id is consumed by accept().
pub fn connect(name: &[u16], client_pid: u64) -> Result<u32, u32> {
    if name.is_empty() || name.len() >= MAX_NAME || client_pid == 0 { return Err(STATUS_INVALID_PARAMETER); }
    let mut g = Guard::acquire();
    let s = g.state();
    let server_port = s.ports.iter().position(|p| p.active && p.listening && p.name_len as usize == name.len() && p.name[..name.len()] == *name).ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
    let slot = s.connections.iter().position(|c| !c.active).ok_or(STATUS_PENDING)?;
    let request_id = alloc_id(s);
    // The request identifier is also the client's endpoint identifier.
    // It becomes usable when the server accepts the request.
    s.connections[slot] = Connection { active: true, accepted: false, request_id, server_port: server_port as u32 + 1, client_pid, server_pid: s.ports[server_port].owner_pid, client_endpoint: request_id, server_endpoint: 0 };
    Ok(request_id)
}

/// Accept or reject a pending request. A rejected request is destroyed.
pub fn accept(port_id: u32, owner_pid: u64, request_id: u32, accept_request: bool) -> Result<u32, u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let port = s.ports.get(port_id.checked_sub(1).ok_or(STATUS_INVALID_HANDLE)? as usize).ok_or(STATUS_INVALID_HANDLE)?;
    if !port.active || !port.listening || port.owner_pid != owner_pid { return Err(STATUS_INVALID_HANDLE); }
    let slot = s.connections.iter().position(|c| c.active && !c.accepted && c.request_id == request_id && c.server_port == port_id).ok_or(STATUS_INVALID_PARAMETER)?;
    if !accept_request { s.connections[slot] = Connection::EMPTY; return Err(STATUS_PORT_DISCONNECTED); }
    let server_endpoint = alloc_id(s);
    s.connections[slot].accepted = true;
    s.connections[slot].server_endpoint = server_endpoint;
    Ok(server_endpoint)
}

/// Send one bounded message to the peer endpoint. No user pointers are retained.
pub fn send(endpoint: u32, sender_pid: u64, bytes: &[u8]) -> Result<(), u32> {
    if bytes.is_empty() || bytes.len() > MAX_MESSAGE_SIZE { return Err(STATUS_INVALID_PARAMETER); }
    let mut g = Guard::acquire();
    let s = g.state();
    let ci = endpoint_connection(s, endpoint).ok_or(STATUS_PORT_DISCONNECTED)?;
    let c = s.connections[ci];
    if !c.accepted { return Err(STATUS_PORT_DISCONNECTED); }
    let (source, destination, expected_pid) = if endpoint == c.client_endpoint {
        (c.client_endpoint, c.server_endpoint, c.client_pid)
    } else if endpoint == c.server_endpoint {
        (c.server_endpoint, c.client_endpoint, c.server_pid)
    } else { return Err(STATUS_INVALID_HANDLE); };
    if sender_pid != expected_pid { return Err(STATUS_INVALID_HANDLE); }
    let port_index = c.server_port.saturating_sub(1) as usize;
    let port_limit = s.ports.get(port_index).filter(|p| p.active).map(|p| p.max_message as usize).unwrap_or(0);
    if bytes.len() > port_limit { return Err(STATUS_BUFFER_TOO_SMALL); }
    let slot = s.messages.iter().position(|m| !m.active).ok_or(STATUS_PENDING)?;
    let mut msg = Message::EMPTY;
    msg.active = true;
    msg.source = source;
    msg.destination = destination;
    msg.length = bytes.len() as u16;
    msg.sequence = s.next_message;
    s.next_message = s.next_message.wrapping_add(1).max(1);
    msg.bytes[..bytes.len()].copy_from_slice(bytes);
    s.messages[slot] = msg;
    Ok(())
}

/// Receive the oldest queued message for endpoint. STATUS_PENDING means wait/retry.
pub fn receive(endpoint: u32, receiver_pid: u64, output: &mut [u8]) -> Result<(usize, u32), u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let ci = endpoint_connection(s, endpoint).ok_or(STATUS_PORT_DISCONNECTED)?;
    let c = s.connections[ci];
    let expected_pid = if endpoint == c.client_endpoint { c.client_pid } else if endpoint == c.server_endpoint { c.server_pid } else { return Err(STATUS_INVALID_HANDLE); };
    if receiver_pid != expected_pid { return Err(STATUS_INVALID_HANDLE); }
    let slot = s.messages.iter().enumerate().filter(|(_, m)| m.active && m.destination == endpoint).min_by_key(|(_, m)| m.sequence).map(|(i, _)| i).ok_or(STATUS_PENDING)?;
    let msg = s.messages[slot];
    if output.len() < msg.length as usize { return Err(STATUS_BUFFER_TOO_SMALL); }
    output[..msg.length as usize].copy_from_slice(&msg.bytes[..msg.length as usize]);
    s.messages[slot] = Message::EMPTY;
    Ok((msg.length as usize, msg.source))
}

/// Tear down both endpoints and discard queued messages.
pub fn disconnect(endpoint: u32, caller_pid: u64) -> Result<(), u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let ci = endpoint_connection(s, endpoint).ok_or(STATUS_PORT_DISCONNECTED)?;
    let c = s.connections[ci];
    if caller_pid != c.client_pid && caller_pid != c.server_pid { return Err(STATUS_INVALID_HANDLE); }
    s.connections[ci] = Connection::EMPTY;
    for m in &mut s.messages { if m.active && (m.source == c.client_endpoint || m.source == c.server_endpoint || m.destination == c.client_endpoint || m.destination == c.server_endpoint) { *m = Message::EMPTY; } }
    Ok(())
}

/// Close a server port and disconnect all clients. This is used by object rundown.
pub fn close_port(port_id: u32, owner_pid: u64) -> Result<(), u32> {
    let mut g = Guard::acquire();
    let s = g.state();
    let index = port_id.checked_sub(1).ok_or(STATUS_INVALID_HANDLE)? as usize;
    let p = *s.ports.get(index).ok_or(STATUS_INVALID_HANDLE)?;
    if !p.active || p.owner_pid != owner_pid { return Err(STATUS_INVALID_HANDLE); }
    for c in &mut s.connections { if c.active && c.server_port == port_id { *c = Connection::EMPTY; } }
    s.ports[index] = Port::EMPTY;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listen_accept_and_bidirectional_messages() {
        let name: Vec<u16> = "Windows\\ApiPort".encode_utf16().collect();
        let port = create_port(&name, 4, 256).unwrap();
        listen(port, 4).unwrap();
        let client_ep = connect(&name, 12).unwrap();
        let request = listen_next(port, 4).unwrap();
        assert_eq!(request, client_ep);
        let server_ep = accept(port, 4, request, true).unwrap();
        let client_ep = {
            let g = Guard::acquire();
            let s = unsafe { &*STATE.state.get() };
            s.connections.iter().find(|c| c.active && c.request_id == request).unwrap().client_endpoint
        };
        send(client_ep, 12, b"hello").unwrap();
        let mut buf = [0u8; 16];
        let (n, from) = receive(server_ep, 4, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello");
        assert_eq!(from, client_ep);
        send(server_ep, 4, b"world").unwrap();
        let (n, _) = receive(client_ep, 12, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"world");
        disconnect(client_ep, 12).unwrap();
    }
    #[test]
    fn names_and_sender_identity_are_checked() {
        let name: Vec<u16> = "CognacTestPort".encode_utf16().collect();
        let port = create_port(&name, 8, 512).unwrap();
        assert_eq!(create_port(&name, 9, 512), Err(STATUS_OBJECT_NAME_COLLISION));
        listen(port, 8).unwrap();
        let client_ep = connect(&name, 10).unwrap();
        let request = listen_next(port, 8).unwrap();
        assert_eq!(request, client_ep);
        let server_ep = accept(port, 8, request, true).unwrap();
        let client_ep = { let g=Guard::acquire(); let s=unsafe{&*STATE.state.get()}; s.connections.iter().find(|c|c.active&&c.request_id==request).unwrap().client_endpoint };
        assert_eq!(send(client_ep, 99, b"forged"), Err(STATUS_INVALID_HANDLE));
        assert_eq!(receive(server_ep, 99, &mut [0u8;8]), Err(STATUS_INVALID_HANDLE));
    }
}
