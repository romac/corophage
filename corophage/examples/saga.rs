//! Saga-style order processing with compensating rollbacks.
//!
//! Demonstrates corophage's capabilities through a multi-step business workflow:
//!
//! - **Multiple effects as workflow steps**: each stage of order processing
//!   (reserve inventory, charge payment, send confirmation, ship order) is a
//!   separate effect, keeping the workflow logic decoupled from service
//!   implementations.
//!
//! - **Stateful handlers**: a `SagaState` struct tracks which steps have
//!   completed, updated by each handler as it runs.
//!
//! - **Cancellation with `Control::cancel()`**: when a step fails, the handler
//!   cancels the computation immediately. The caller then inspects the
//!   accumulated state and runs compensating actions in reverse order,
//!   implementing the saga rollback pattern.
//!
//! - **Async handlers**: all handlers are async, as they would be in a real
//!   service-oriented architecture.
//!
//! Run with: `cargo run --example saga`

use corophage::prelude::*;

// ── Effects ─────────────────────────────────────────────────────────────────

/// Reserve `quantity` units of an item. Resumes with a reservation ID.
#[effect(String)]
struct ReserveInventory {
    item_id: String,
    quantity: u32,
}

/// Charge a payment method. Resumes with a transaction ID.
#[effect(String)]
struct ChargePayment {
    amount_cents: u64,
    card_token: String,
}

/// Send an order confirmation email. Resumes with `()`.
#[effect(())]
struct SendConfirmation {
    email: String,
    order_id: String,
}

/// Ship the order. Resumes with a tracking number.
#[effect(String)]
struct ShipOrder {
    order_id: String,
    address: String,
}

// ── Order ───────────────────────────────────────────────────────────────────

struct Order {
    item_id: String,
    quantity: u32,
    amount_cents: u64,
    card_token: String,
    email: String,
    address: String,
}

// ── Saga state ──────────────────────────────────────────────────────────────

/// Tracks which saga steps have completed, enabling targeted compensation.
#[derive(Default)]
struct SagaState {
    reservation_id: Option<String>,
    transaction_id: Option<String>,
    confirmation_sent: bool,
    tracking_number: Option<String>,
    failure_reason: Option<String>,
    /// Which step to simulate a failure on (for demonstration).
    fail_at: Option<String>,
}

impl SagaState {
    /// Run compensating actions for completed steps, in reverse order.
    fn compensate(&self) {
        println!("  Running compensations...");

        if self.confirmation_sent {
            println!("    Sending cancellation notice to customer");
        }

        if let Some(tx_id) = &self.transaction_id {
            println!("    Refunding transaction {tx_id}");
        }

        if let Some(res_id) = &self.reservation_id {
            println!("    Releasing inventory reservation {res_id}");
        }

        println!("  Compensations complete.");
    }
}

// ── Workflow ────────────────────────────────────────────────────────────────

#[effectful(ReserveInventory, ChargePayment, SendConfirmation, ShipOrder)]
fn process_order(order: Order) -> String {
    // Step 1: Reserve inventory
    let reservation_id = yield_!(ReserveInventory {
        item_id: order.item_id,
        quantity: order.quantity,
    });
    let order_id = format!("ORD-{}", &reservation_id[..8]);
    println!("    Reserved inventory: {reservation_id}");

    // Step 2: Charge payment
    let transaction_id = yield_!(ChargePayment {
        amount_cents: order.amount_cents,
        card_token: order.card_token,
    });
    println!("    Payment charged: {transaction_id}");

    // Step 3: Send confirmation
    yield_!(SendConfirmation {
        email: order.email,
        order_id: order_id.clone(),
    });
    println!("    Confirmation sent");

    // Step 4: Ship order
    let tracking_number = yield_!(ShipOrder {
        order_id: order_id.clone(),
        address: order.address,
    });
    println!("    Shipped: {tracking_number}");

    format!("Order {order_id} complete! Tracking: {tracking_number}, Payment: {transaction_id}")
}

// ── Handlers ────────────────────────────────────────────────────────────────

async fn handle_reserve(state: &mut SagaState, effect: ReserveInventory) -> Control<String> {
    if state.fail_at.as_deref() == Some("inventory") {
        state.failure_reason = Some(format!("Item {} is out of stock", effect.item_id));
        return Control::cancel();
    }

    let id = format!(
        "RES-{:08x}-{}",
        fxhash(effect.item_id.as_bytes()),
        effect.quantity
    );
    state.reservation_id = Some(id.clone());
    Control::resume(id)
}

async fn handle_payment(state: &mut SagaState, effect: ChargePayment) -> Control<String> {
    if state.fail_at.as_deref() == Some("payment") {
        state.failure_reason = Some(format!(
            "Card {} declined for {} cents",
            effect.card_token, effect.amount_cents
        ));
        return Control::cancel();
    }

    let id = format!("TXN-{:08x}", fxhash(effect.card_token.as_bytes()));
    state.transaction_id = Some(id.clone());
    Control::resume(id)
}

async fn handle_confirmation(state: &mut SagaState, effect: SendConfirmation) -> Control<()> {
    if state.fail_at.as_deref() == Some("confirmation") {
        state.failure_reason = Some(format!(
            "Failed to send confirmation for {} to {}",
            effect.order_id, effect.email
        ));
        return Control::cancel();
    }

    state.confirmation_sent = true;
    Control::resume(())
}

async fn handle_shipping(state: &mut SagaState, effect: ShipOrder) -> Control<String> {
    if state.fail_at.as_deref() == Some("shipping") {
        state.failure_reason = Some(format!("No carriers available for {}", effect.address));
        return Control::cancel();
    }

    let id = format!("TRK-{:08x}", fxhash(effect.order_id.as_bytes()));
    state.tracking_number = Some(id.clone());
    Control::resume(id)
}

// ── Scenario runner ─────────────────────────────────────────────────────────

async fn run_scenario(name: &str, fail_at: Option<&str>) {
    println!("\n--- {name} ---\n");

    let order = Order {
        item_id: "WIDGET-42".into(),
        quantity: 3,
        amount_cents: 4999,
        card_token: "tok_visa_1234".into(),
        email: "alice@example.com".into(),
        address: "123 Main St, Springfield".into(),
    };

    let mut state = SagaState {
        fail_at: fail_at.map(Into::into),
        ..Default::default()
    };

    let result = process_order(order)
        .handle(handle_reserve)
        .handle(handle_payment)
        .handle(handle_confirmation)
        .handle(handle_shipping)
        .run_stateful(&mut state)
        .await;

    match result {
        Ok(summary) => {
            println!("\n  SUCCESS: {summary}");
        }
        Err(_cancelled) => {
            let reason = state.failure_reason.as_deref().unwrap_or("unknown");
            println!("\n  FAILED: {reason}");
            state.compensate();
        }
    }
}

// ── Main ────────────────────────────────────────────────────────────────────

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("=== Order Processing Saga ===");

    // Happy path: all steps succeed
    run_scenario("Happy path", None).await;

    // Payment fails after inventory is reserved
    run_scenario("Payment declined", Some("payment")).await;

    // Shipping fails after payment and confirmation
    run_scenario("Shipping unavailable", Some("shipping")).await;

    // First step fails, nothing to compensate
    run_scenario("Out of stock", Some("inventory")).await;

    println!();
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Simple FNV-1a hash for generating deterministic fake IDs.
fn fxhash(data: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for &byte in data {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[cfg(test)]
fn test_order() -> Order {
    Order {
        item_id: "WIDGET-42".into(),
        quantity: 3,
        amount_cents: 4999,
        card_token: "tok_visa_1234".into(),
        email: "alice@example.com".into(),
        address: "123 Main St, Springfield".into(),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────
//
// These tests demonstrate how to use mock handlers with corophage to write
// deterministic, isolated tests for effectful workflows. Because effects
// decouple the workflow logic from its side effects, you can swap in mock
// handlers that return canned responses, assert on received effect payloads,
// or simulate failures -- no real services needed.

#[cfg(test)]
mod tests {
    use super::*;

    // ── Mock handler helpers ────────────────────────────────────────────
    //
    // Because effects decouple the workflow from its side effects, testing
    // is straightforward: swap in mock handlers that return canned values,
    // simulate failures, or record what they received via shared state.

    /// Attach mock handlers that always succeed with predictable IDs.
    async fn run_with_mock_handlers(order: Order) -> Result<String, Cancelled> {
        let mut state = SagaState::default();

        process_order(order)
            .handle(
                async |state: &mut SagaState, effect: ReserveInventory| -> Control<String> {
                    let id = format!("MOCK-RES-{}-{}", effect.item_id, effect.quantity);
                    state.reservation_id = Some(id.clone());
                    Control::resume(id)
                },
            )
            .handle(
                async |state: &mut SagaState, effect: ChargePayment| -> Control<String> {
                    let id = format!("MOCK-TXN-{}", effect.amount_cents);
                    state.transaction_id = Some(id.clone());
                    Control::resume(id)
                },
            )
            .handle(
                async |state: &mut SagaState, _effect: SendConfirmation| -> Control<()> {
                    state.confirmation_sent = true;
                    Control::resume(())
                },
            )
            .handle(
                async |state: &mut SagaState, effect: ShipOrder| -> Control<String> {
                    let id = format!("MOCK-TRK-{}", effect.order_id);
                    state.tracking_number = Some(id.clone());
                    Control::resume(id)
                },
            )
            .run_stateful(&mut state)
            .await
    }

    // ── Happy path ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn happy_path_returns_summary_with_ids() {
        let result = run_with_mock_handlers(test_order()).await.unwrap();

        // The workflow formats the order ID from the first 8 chars of the reservation ID.
        assert!(result.contains("ORD-MOCK-RES"), "expected order ID prefix");
        assert!(result.contains("MOCK-TXN-4999"), "expected transaction ID");
        assert!(result.contains("MOCK-TRK-"), "expected tracking number");
    }

    // ── Cancellation: payment failure ───────────────────────────────────

    #[tokio::test]
    async fn payment_failure_cancels_and_records_state() {
        let mut state = SagaState::default();

        let result = process_order(test_order())
            .handle(
                async |state: &mut SagaState, effect: ReserveInventory| -> Control<String> {
                    let id = format!("RES-{}", effect.item_id);
                    state.reservation_id = Some(id.clone());
                    Control::resume(id)
                },
            )
            // Mock payment handler that always declines
            .handle(
                async |_state: &mut SagaState, _: ChargePayment| -> Control<String> {
                    Control::cancel()
                },
            )
            // These handlers should never be reached
            .handle(
                async |_: &mut SagaState, _: SendConfirmation| -> Control<()> {
                    panic!("confirmation handler should not be called after cancel")
                },
            )
            .handle(async |_: &mut SagaState, _: ShipOrder| -> Control<String> {
                panic!("shipping handler should not be called after cancel")
            })
            .run_stateful(&mut state)
            .await;

        assert_eq!(result, Err(Cancelled));
        // Inventory was reserved before the failure
        assert_eq!(state.reservation_id, Some("RES-WIDGET-42".into()));
        // Nothing after payment should have executed
        assert_eq!(state.transaction_id, None);
        assert!(!state.confirmation_sent);
        assert_eq!(state.tracking_number, None);
    }

    // ── Cancellation: shipping failure ──────────────────────────────────

    #[tokio::test]
    async fn shipping_failure_preserves_all_prior_state() {
        let mut state = SagaState::default();

        let result = process_order(test_order())
            .handle(
                async |state: &mut SagaState, _: ReserveInventory| -> Control<String> {
                    state.reservation_id = Some("RES-00001".into());
                    Control::resume("RES-00001".into())
                },
            )
            .handle(
                async |state: &mut SagaState, _: ChargePayment| -> Control<String> {
                    state.transaction_id = Some("TXN-00001".into());
                    Control::resume("TXN-00001".into())
                },
            )
            .handle(
                async |state: &mut SagaState, _: SendConfirmation| -> Control<()> {
                    state.confirmation_sent = true;
                    Control::resume(())
                },
            )
            // Shipping always fails
            .handle(async |_: &mut SagaState, _: ShipOrder| -> Control<String> {
                Control::cancel()
            })
            .run_stateful(&mut state)
            .await;

        assert_eq!(result, Err(Cancelled));
        // All steps before shipping completed and are recorded
        assert!(state.reservation_id.is_some());
        assert!(state.transaction_id.is_some());
        assert!(state.confirmation_sent);
        // Shipping did not complete
        assert_eq!(state.tracking_number, None);
    }

    // ── Asserting on effect payloads ────────────────────────────────────
    //
    // To verify that the workflow sends the right data to each effect,
    // extend the shared state with extra fields to capture payloads.
    // Handlers are AsyncFn (not AsyncFnMut), so captured locals can't be
    // mutated -- but &mut State can, which is the idiomatic approach.

    #[derive(Default)]
    struct PayloadCapture {
        saga: SagaState,
        seen_item_id: String,
        seen_quantity: u32,
        seen_amount: u64,
        seen_email: String,
        seen_address: String,
    }

    #[tokio::test]
    async fn handlers_receive_correct_effect_payloads() {
        let mut capture = PayloadCapture::default();

        let _ = process_order(test_order())
            .handle(
                async |s: &mut PayloadCapture, effect: ReserveInventory| -> Control<String> {
                    s.seen_item_id = effect.item_id.clone();
                    s.seen_quantity = effect.quantity;
                    s.saga.reservation_id = Some("R1".into());
                    Control::resume("R1234567-rest".into())
                },
            )
            .handle(
                async |s: &mut PayloadCapture, effect: ChargePayment| -> Control<String> {
                    s.seen_amount = effect.amount_cents;
                    s.saga.transaction_id = Some("T1".into());
                    Control::resume("T1".into())
                },
            )
            .handle(
                async |s: &mut PayloadCapture, effect: SendConfirmation| -> Control<()> {
                    s.seen_email = effect.email.clone();
                    s.saga.confirmation_sent = true;
                    Control::resume(())
                },
            )
            .handle(
                async |s: &mut PayloadCapture, effect: ShipOrder| -> Control<String> {
                    s.seen_address = effect.address.clone();
                    s.saga.tracking_number = Some("TRK-1".into());
                    Control::resume("TRK-1".into())
                },
            )
            .run_stateful(&mut capture)
            .await;

        assert_eq!(capture.seen_item_id, "WIDGET-42");
        assert_eq!(capture.seen_quantity, 3);
        assert_eq!(capture.seen_amount, 4999);
        assert_eq!(capture.seen_email, "alice@example.com");
        assert_eq!(capture.seen_address, "123 Main St, Springfield");
    }

    // ── First step failure: nothing to compensate ───────────────────────

    #[tokio::test]
    async fn inventory_failure_leaves_clean_state() {
        let mut state = SagaState::default();

        let result = process_order(test_order())
            .handle(
                async |_: &mut SagaState, _: ReserveInventory| -> Control<String> {
                    Control::cancel()
                },
            )
            .handle(
                async |_: &mut SagaState, _: ChargePayment| -> Control<String> {
                    panic!("should not be reached")
                },
            )
            .handle(
                async |_: &mut SagaState, _: SendConfirmation| -> Control<()> {
                    panic!("should not be reached")
                },
            )
            .handle(async |_: &mut SagaState, _: ShipOrder| -> Control<String> {
                panic!("should not be reached")
            })
            .run_stateful(&mut state)
            .await;

        assert_eq!(result, Err(Cancelled));
        // Nothing was recorded
        assert!(state.reservation_id.is_none());
        assert!(state.transaction_id.is_none());
        assert!(!state.confirmation_sent);
        assert!(state.tracking_number.is_none());
    }

    // ── Stateless mock handlers (no shared state) ───────────────────────

    #[tokio::test]
    async fn works_with_stateless_handlers_too() {
        // You don't need shared state at all if you just want to test the
        // workflow's return value. Plain closures work with .run().
        let result = process_order(test_order())
            .handle(async |_: ReserveInventory| -> Control<String> {
                Control::resume("RES-FAKE-0".into())
            })
            .handle(async |_: ChargePayment| -> Control<String> {
                Control::resume("TXN-FAKE".into())
            })
            .handle(async |_: SendConfirmation| -> Control<()> { Control::resume(()) })
            .handle(async |_: ShipOrder| -> Control<String> { Control::resume("TRK-FAKE".into()) })
            .run()
            .await;

        let summary = result.unwrap();
        assert!(summary.contains("ORD-RES-FAKE"));
        assert!(summary.contains("TRK-FAKE"));
    }
}
