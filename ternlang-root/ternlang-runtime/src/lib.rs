//! ternlang-runtime — Distributed actor runtime for ternlang
//!
//! Phase 5.1: synchronous TCP transport for remote agent communication.
//!
//! Protocol: newline-delimited JSON over TCP.
//! Each message is a single JSON object followed by '\n'.
//!
//! Message types:
//!   {"type":"send",  "agent_id": 0, "trit": 1}     → send trit to local agent
//!   {"type":"await", "agent_id": 0}                 → run agent handler, return result
//!   {"type":"reply", "trit": 1}                     → response to await
//!   {"type":"error", "msg": "..."}                  → error response
//!
//! Usage:
//!   let node = TernNode::new("127.0.0.1:7373");
//!   node.listen();                  // spawns listener thread
//!   node.connect("127.0.0.1:7374"); // connect to peer
//!   node.remote_send("127.0.0.1:7374", 0, 1);  // send +1 to remote agent 0
//!   let result = node.remote_await("127.0.0.1:7374", 0); // get reply

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};

/// A trit value serialized over the wire: -1, 0, or +1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireTrit(pub i8);

impl WireTrit {
    pub fn new(v: i8) -> Self {
        assert!(v == -1 || v == 0 || v == 1, "invalid trit: {}", v);
        WireTrit(v)
    }
}

/// Wire protocol message.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TernMessage {
    /// Send a trit message to a local agent's mailbox.
    Send  { agent_id: usize, trit: i8 },
    /// Execute the agent's handler with its pending message, return the result.
    Await { agent_id: usize },
    /// Successful reply to an Await.
    Reply { trit: i8 },
    /// Error response.
    Error { msg: String },
}

/// A remote agent reference: identifies an agent on a specific node.
#[derive(Debug, Clone)]
pub struct RemoteAgentRef {
    pub node_addr: String,
    pub agent_id: usize,
}

/// Local agent record: mailbox of pending trit messages.
#[derive(Debug, Default)]
struct LocalAgent {
    mailbox: std::collections::VecDeque<i8>,
}

/// The ternlang distributed node.
/// Manages local agent mailboxes and TCP connections to peer nodes.
pub struct TernNode {
    pub addr: String,
    /// Local agents indexed by agent_id.
    agents: Arc<Mutex<HashMap<usize, LocalAgent>>>,
    /// Open connections to peer nodes: addr → stream.
    peers: Arc<Mutex<HashMap<String, TcpStream>>>,
}

impl TernNode {
    pub fn new(addr: &str) -> Self {
        TernNode {
            addr: addr.to_string(),
            agents: Arc::new(Mutex::new(HashMap::new())),
            peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a local agent so it can receive remote messages.
    pub fn register_agent(&self, agent_id: usize) {
        self.agents.lock().unwrap()
            .entry(agent_id)
            .or_default();
    }

    /// Start the TCP listener in a background thread.
    /// Incoming messages are dispatched to local agent mailboxes.
    pub fn listen(&self) {
        let addr = self.addr.clone();
        let agents = Arc::clone(&self.agents);

        thread::spawn(move || {
            let listener = TcpListener::bind(&addr)
                .unwrap_or_else(|e| panic!("TernNode: cannot bind {}: {}", addr, e));
            for stream in listener.incoming().flatten() {
                let agents = Arc::clone(&agents);
                thread::spawn(move || {
                    handle_connection(stream, agents);
                });
            }
        });
    }

    /// Connect to a peer node, storing the stream for future sends.
    pub fn connect(&self, peer_addr: &str) -> std::io::Result<()> {
        let stream = TcpStream::connect(peer_addr)?;
        self.peers.lock().unwrap()
            .insert(peer_addr.to_string(), stream);
        Ok(())
    }

    /// Send a trit to a remote agent's mailbox.
    pub fn remote_send(&self, peer_addr: &str, agent_id: usize, trit: i8) -> std::io::Result<()> {
        let msg = TernMessage::Send { agent_id, trit };
        self.send_msg(peer_addr, &msg)
    }

    /// Trigger a remote agent to process its mailbox and return the result trit.
    pub fn remote_await(&self, peer_addr: &str, agent_id: usize) -> std::io::Result<i8> {
        let msg = TernMessage::Await { agent_id };
        self.send_msg(peer_addr, &msg)?;
        // Read the reply from the same connection.
        let mut peers = self.peers.lock().unwrap();
        let stream = peers.get_mut(peer_addr)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotConnected, "not connected"))?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let reply: TernMessage = serde_json::from_str(line.trim())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        match reply {
            TernMessage::Reply { trit } => Ok(trit),
            TernMessage::Error { msg } =>
                Err(std::io::Error::new(std::io::ErrorKind::Other, msg)),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unexpected message")),
        }
    }

    /// Push a trit directly into a local agent's mailbox (no network).
    pub fn local_send(&self, agent_id: usize, trit: i8) {
        let mut agents = self.agents.lock().unwrap();
        agents.entry(agent_id).or_default().mailbox.push_back(trit);
    }

    /// Pop a trit from a local agent's mailbox (returns 0 if empty).
    pub fn local_pop(&self, agent_id: usize) -> i8 {
        let mut agents = self.agents.lock().unwrap();
        agents.entry(agent_id).or_default().mailbox.pop_front().unwrap_or(0)
    }

    fn send_msg(&self, peer_addr: &str, msg: &TernMessage) -> std::io::Result<()> {
        let mut peers = self.peers.lock().unwrap();
        let stream = peers.get_mut(peer_addr)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotConnected, "not connected"))?;
        let mut line = serde_json::to_string(msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        line.push('\n');
        stream.write_all(line.as_bytes())
    }
}

/// Handle one incoming connection — reads messages, writes replies.
/// The caller supplies a handler function for Await messages.
/// For Phase 5.1 the Await handler is the identity (echoes mailbox message back).
fn handle_connection(stream: TcpStream, agents: Arc<Mutex<HashMap<usize, LocalAgent>>>) {
    let mut writer = stream.try_clone().expect("clone failed");
    let reader = BufReader::new(stream);
    for line in reader.lines().flatten() {
        let msg: TernMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                let err = TernMessage::Error { msg: e.to_string() };
                let _ = writeln!(writer, "{}", serde_json::to_string(&err).unwrap());
                continue;
            }
        };
        match msg {
            TernMessage::Send { agent_id, trit } => {
                agents.lock().unwrap()
                    .entry(agent_id)
                    .or_default()
                    .mailbox.push_back(trit);
                // No reply expected for Send.
            }
            TernMessage::Await { agent_id } => {
                let trit = agents.lock().unwrap()
                    .entry(agent_id)
                    .or_default()
                    .mailbox.pop_front()
                    .unwrap_or(0); // empty mailbox → hold (0)
                let reply = TernMessage::Reply { trit };
                let _ = writeln!(writer, "{}", serde_json::to_string(&reply).unwrap());
            }
            _ => {
                let err = TernMessage::Error { msg: "unexpected message type".into() };
                let _ = writeln!(writer, "{}", serde_json::to_string(&err).unwrap());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_local_send_pop() {
        let node = TernNode::new("127.0.0.1:0"); // port 0 = don't listen
        node.register_agent(0);
        node.local_send(0, 1);
        node.local_send(0, -1);
        assert_eq!(node.local_pop(0),  1);
        assert_eq!(node.local_pop(0), -1);
        assert_eq!(node.local_pop(0),  0); // empty → hold
    }

    #[test]
    fn test_wire_protocol_send_await() {
        // Start a listener node on a free port
        let server = TernNode::new("127.0.0.1:7373");
        server.register_agent(42);
        server.listen();
        thread::sleep(Duration::from_millis(50)); // let listener start

        // Client connects and sends a trit to agent 42, then awaits
        let client = TernNode::new("127.0.0.1:0");
        client.connect("127.0.0.1:7373").expect("connect failed");
        client.remote_send("127.0.0.1:7373", 42, 1).expect("send failed");

        // Now await — server pops mailbox (holds +1) and replies
        let result = client.remote_await("127.0.0.1:7373", 42).expect("await failed");
        assert_eq!(result, 1);
    }
}
