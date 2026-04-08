//! Choreographic programming via algebraic effects.
//!
//! Implements the core ideas from "Toward Verified Library-Level Choreographic
//! Programming with Algebraic Effects" (Shen & Kuper, 2024): a single global
//! choreography is written once, then *projected* to each participant by
//! swapping in location-specific effect handlers.
//!
//! ## Background
//!
//! In choreographic programming (CP) a distributed protocol is expressed as one
//! unified program -- the **choreography** -- rather than as separate per-node
//! programs. A compilation step called **endpoint projection** (EPP) extracts
//! each node's individual behavior from the choreography.
//!
//! With algebraic effects the mapping is direct:
//!
//!   - A choreography is an effectful computation.
//!   - EPP is handler selection: each location provides its own handlers that
//!     interpret effects from that node's perspective.
//!   - **Located values** (values that "live" at a specific node) are modeled by
//!     returning the real value at the owning location and an empty placeholder
//!     elsewhere.
//!
//! ## The pipeline
//!
//! ```text
//!   input    <- locally Alice, "the quick brown fox ..."
//!   at_bob   <- send Alice => Bob, input
//!   count    <- locally Bob, countWords(at_bob)
//!   at_carol <- send Bob => Carol, count
//!   report   <- locally Carol, formatReport(at_carol)
//!   result   <- send Carol => Alice, report
//! ```
//!
//! All three locations run the **same** `pipeline()` function. The handlers
//! decide, for each effect, whether the current node is the actor, a passive
//! participant, or uninvolved.
//!
//! Run with: `cargo run --example choreography`

use std::collections::HashMap;

use corophage::prelude::*;
use tokio::sync::mpsc;

// ── Locations ──────────────────────────────────────────────────────────────

/// A node in the distributed system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Loc {
    Alice,
    Bob,
    Carol,
}

impl std::fmt::Display for Loc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Loc::Alice => write!(f, "Alice"),
            Loc::Bob => write!(f, "Bob"),
            Loc::Carol => write!(f, "Carol"),
        }
    }
}

// ── Choreography effects ───────────────────────────────────────────────────

/// A local computation at a given location.
///
/// The `value` field carries the eagerly-computed result. The handler at the
/// owning location resumes with this value; everywhere else it resumes with
/// an empty string (the "erased" located value from the paper).
///
/// Because the choreography runs at every location, the expression that
/// produces `value` is evaluated everywhere. At non-owning locations the
/// inputs are placeholders, so the result is meaningless, but that is fine
/// because the handler discards it.
#[effect(String)]
struct Locally {
    at: Loc,
    value: String,
}

/// Communicate a value from one location to another.
///
/// The `payload` is meaningful only at the sender. The handler at the sender
/// transmits it over a channel; at the receiver it reads from the channel.
/// All other locations resume with an empty placeholder.
#[effect(String)]
struct Comm {
    from: Loc,
    to: Loc,
    payload: String,
}

// ── The choreography ───────────────────────────────────────────────────────

/// A distributed data-processing pipeline, written as a single choreography.
///
/// Alice provides input text, Bob counts words, Carol formats a report.
/// All computation is expressed inline; there are no external "compute"
/// callbacks.
#[effectful(Locally, Comm)]
fn pipeline() -> String {
    use Loc::*;

    macro_rules! locally {
        ($loc:expr, $value:expr) => {
            yield_!(Locally {
                at: $loc,
                value: $value,
            })
        };
    }

    macro_rules! send {
        ($from:expr => $to:expr, $val:expr) => {
            yield_!(Comm {
                from: $from,
                to: $to,
                payload: $val,
            })
        };
    }

    // Alice produces the input text.
    let input = locally!(Alice, "the quick brown fox jumps over the lazy dog".into());

    // Alice sends the raw text to Bob.
    let at_bob = send!(Alice => Bob, input);

    // Bob counts words.
    let count = locally!(Bob, at_bob.split_whitespace().count().to_string());

    // Bob sends the count to Carol.
    let at_carol = send!(Bob => Carol, count);

    // Carol formats a report.
    let report = locally!(Carol, format!("=== Report: {at_carol} words ==="));

    // Carol sends the finished report back to Alice.
    send!(Carol => Alice, report)
}

// ── Per-node state ─────────────────────────────────────────────────────────

/// Runtime state for one projected node: identity + channel endpoints.
struct NodeState {
    loc: Loc,
    senders: HashMap<Loc, mpsc::Sender<String>>,
    receivers: HashMap<Loc, mpsc::Receiver<String>>,
}

impl NodeState {
    async fn send_to(&self, dest: Loc, msg: String) {
        self.senders[&dest].send(msg).await.expect("channel closed");
    }

    async fn recv_from(&mut self, source: Loc) -> String {
        self.receivers
            .get_mut(&source)
            .expect("no channel from source")
            .recv()
            .await
            .expect("channel closed")
    }
}

// ── Endpoint projection (handlers) ─────────────────────────────────────────

/// EPP handler for `Locally`: keeps the pre-computed value at the owning
/// node, erases it everywhere else.
async fn handle_locally(state: &mut NodeState, eff: Locally) -> Control<String> {
    if eff.at == state.loc {
        println!("  [{}] local: {:?}", state.loc, eff.value);
        Control::resume(eff.value)
    } else {
        Control::resume(String::new())
    }
}

/// EPP handler for `Comm`: sends, receives, or ignores depending on our role.
///
/// This corresponds to the `epp`/`alg` function in Shen & Kuper (2024), Fig. 3:
///   - `from == me`:  transmit the payload and resume with a placeholder
///   - `to == me`:    receive from the channel and resume with the value
///   - otherwise:     resume with a placeholder (not involved)
async fn handle_comm(state: &mut NodeState, eff: Comm) -> Control<String> {
    let me = state.loc;

    if eff.from == me {
        println!("  [{me}] send to {}: {:?}", eff.to, eff.payload);
        state.send_to(eff.to, eff.payload).await;
        Control::resume(String::new())
    } else if eff.to == me {
        let value = state.recv_from(eff.from).await;
        println!("  [{me}] recv from {}: {:?}", eff.from, value);
        Control::resume(value)
    } else {
        // Not involved in this communication step.
        Control::resume(String::new())
    }
}

// ── Channel mesh ───────────────────────────────────────────────────────────

type SenderMap = HashMap<Loc, HashMap<Loc, mpsc::Sender<String>>>;
type ReceiverMap = HashMap<Loc, HashMap<Loc, mpsc::Receiver<String>>>;

/// Build a full mesh of channels between all locations.
fn channel_mesh(locs: &[Loc]) -> (SenderMap, ReceiverMap) {
    let mut senders: HashMap<Loc, HashMap<Loc, mpsc::Sender<String>>> = HashMap::new();
    let mut receivers: HashMap<Loc, HashMap<Loc, mpsc::Receiver<String>>> = HashMap::new();

    for &from in locs {
        for &to in locs {
            if from != to {
                let (tx, rx) = mpsc::channel(1);
                senders.entry(from).or_default().insert(to, tx);
                receivers.entry(to).or_default().insert(from, rx);
            }
        }
    }

    (senders, receivers)
}

/// Create node states for all locations.
fn make_nodes(locs: &[Loc]) -> Vec<NodeState> {
    let (mut senders, mut receivers) = channel_mesh(locs);

    locs.iter()
        .map(|&loc| NodeState {
            loc,
            senders: senders.remove(&loc).unwrap_or_default(),
            receivers: receivers.remove(&loc).unwrap_or_default(),
        })
        .collect()
}

// ── Main ───────────────────────────────────────────────────────────────────

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("=== Choreographic Data Pipeline ===\n");
    println!("Choreography (global view):\n");
    println!("  input    <- locally Alice, \"the quick brown fox ...\"");
    println!("  at_bob   <- send Alice => Bob, input");
    println!("  count    <- locally Bob, countWords(at_bob)");
    println!("  at_carol <- send Bob => Carol, count");
    println!("  report   <- locally Carol, formatReport(at_carol)");
    println!("  result   <- send Carol => Alice, report");
    println!();

    let mut nodes = make_nodes(&[Loc::Alice, Loc::Bob, Loc::Carol]);

    let [alice, bob, carol] = nodes.as_mut_slice() else {
        unreachable!()
    };

    // Run the SAME choreography at all three locations concurrently.
    // Only the handler (endpoint projection) differs.
    println!("Running endpoint projections concurrently:\n");

    let (a, b, c) = tokio::join!(
        pipeline()
            .handle(handle_locally)
            .handle(handle_comm)
            .run_stateful(alice),
        pipeline()
            .handle(handle_locally)
            .handle(handle_comm)
            .run_stateful(bob),
        pipeline()
            .handle(handle_locally)
            .handle(handle_comm)
            .run_stateful(carol),
    );

    println!();
    println!("Results (each location ran the same choreography):");
    println!("  Alice: {:?}  <-- the final report", a.unwrap());
    println!("  Bob:   {:?}  <-- erased (not at Alice)", b.unwrap());
    println!("  Carol: {:?}  <-- erased (not at Alice)", c.unwrap());
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Full pipeline with real channels.
    #[tokio::test]
    async fn full_pipeline_produces_correct_results() {
        let mut nodes = make_nodes(&[Loc::Alice, Loc::Bob, Loc::Carol]);

        let [alice, bob, carol] = nodes.as_mut_slice() else {
            unreachable!()
        };

        let (a, b, c) = tokio::join!(
            pipeline()
                .handle(handle_locally)
                .handle(handle_comm)
                .run_stateful(alice),
            pipeline()
                .handle(handle_locally)
                .handle(handle_comm)
                .run_stateful(bob),
            pipeline()
                .handle(handle_locally)
                .handle(handle_comm)
                .run_stateful(carol),
        );

        // Alice receives the final report.
        assert_eq!(a.unwrap(), "=== Report: 9 words ===");
        // Bob and Carol get erased located values (empty strings).
        assert_eq!(b.unwrap(), "");
        assert_eq!(c.unwrap(), "");
    }

    /// Mock EPP for Alice (no channels needed).
    #[tokio::test]
    async fn mock_epp_for_alice() {
        let result = pipeline()
            .handle(async |eff: Locally| -> Control<String> {
                if eff.at == Loc::Alice {
                    Control::resume(eff.value)
                } else {
                    Control::resume(String::new())
                }
            })
            .handle(async |eff: Comm| -> Control<String> {
                if eff.from == Loc::Alice {
                    Control::resume(String::new())
                } else if eff.to == Loc::Alice {
                    // Simulate receiving the final report.
                    Control::resume("=== Report: 9 words ===".into())
                } else {
                    Control::resume(String::new())
                }
            })
            .run()
            .await
            .unwrap();

        assert_eq!(result, "=== Report: 9 words ===");
    }

    /// Mock EPP for Bob: receives text, counts words, sends count.
    #[tokio::test]
    async fn mock_epp_for_bob() {
        let result = pipeline()
            .handle(async |eff: Locally| -> Control<String> {
                if eff.at == Loc::Bob {
                    Control::resume(eff.value)
                } else {
                    Control::resume(String::new())
                }
            })
            .handle(async |eff: Comm| -> Control<String> {
                if eff.to == Loc::Bob {
                    // Bob receives text from Alice.
                    Control::resume("some words here".into())
                } else if eff.from == Loc::Bob {
                    // Bob sends count to Carol.
                    assert_eq!(eff.payload, "3", "Bob should send the word count");
                    Control::resume(String::new())
                } else {
                    Control::resume(String::new())
                }
            })
            .run()
            .await
            .unwrap();

        // Bob's final result is a placeholder (the result lives at Alice).
        assert_eq!(result, "");
    }
}
