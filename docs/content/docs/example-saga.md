+++
title = "Example: Order Saga"
weight = 8
description = "A multi-step workflow with stateful handlers and compensating rollbacks on failure."
+++

The [saga example](https://github.com/romac/corophage/blob/main/corophage/examples/saga.rs) models order processing as a sequence of effects — each step a service call — with automatic rollback when any step fails.

Run it with:

```
cargo run --example saga
```

## The pattern

A saga is a multi-step workflow where each step can fail. When a step fails, all previously completed steps must be *compensated* (undone) in reverse order. Effects are a natural fit: each step is an effect, a shared `SagaState` struct records what completed, and `Control::cancel()` halts the workflow immediately. The caller then reads the state and runs compensations.

## Effects

Each stage of the workflow is its own effect:

```rust
/// Reserve `quantity` units of an item. Resumes with a reservation ID.
#[effect(String)]
struct ReserveInventory { item_id: String, quantity: u32 }

/// Charge a payment method. Resumes with a transaction ID.
#[effect(String)]
struct ChargePayment { amount_cents: u64, card_token: String }

/// Send an order confirmation email. Resumes with `()`.
#[effect(())]
struct SendConfirmation { email: String, order_id: String }

/// Ship the order. Resumes with a tracking number.
#[effect(String)]
struct ShipOrder { order_id: String, address: String }
```

The workflow logic doesn't know about services, retries, or failure modes — it just yields effects.

## Workflow

```rust
#[effectful(ReserveInventory, ChargePayment, SendConfirmation, ShipOrder)]
fn process_order(order: Order) -> String {
    let reservation_id = yield_!(ReserveInventory {
        item_id: order.item_id,
        quantity: order.quantity,
    });
    let order_id = format!("ORD-{}", &reservation_id[..8]);

    let transaction_id = yield_!(ChargePayment {
        amount_cents: order.amount_cents,
        card_token: order.card_token,
    });

    yield_!(SendConfirmation {
        email: order.email,
        order_id: order_id.clone(),
    });

    let tracking_number = yield_!(ShipOrder {
        order_id: order_id.clone(),
        address: order.address,
    });

    format!("Order {order_id} complete! Tracking: {tracking_number}")
}
```

## Tracking state

`SagaState` records what has completed and exposes a `compensate` method that undoes steps in reverse order:

```rust
#[derive(Default)]
struct SagaState {
    reservation_id: Option<String>,
    transaction_id: Option<String>,
    confirmation_sent: bool,
    tracking_number: Option<String>,
    failure_reason: Option<String>,
}

impl SagaState {
    fn compensate(&self) {
        if self.confirmation_sent {
            println!("Sending cancellation notice to customer");
        }
        if let Some(tx_id) = &self.transaction_id {
            println!("Refunding transaction {tx_id}");
        }
        if let Some(res_id) = &self.reservation_id {
            println!("Releasing inventory reservation {res_id}");
        }
    }
}
```

## Handlers

Each handler updates the state, then either resumes or cancels:

```rust
async fn handle_payment(state: &mut SagaState, effect: ChargePayment) -> Control<String> {
    if state.fail_at.as_deref() == Some("payment") {
        state.failure_reason = Some(format!(
            "Card {} declined for {} cents",
            effect.card_token, effect.amount_cents
        ));
        return Control::cancel();
    }

    let id = format!("TXN-{:08x}", hash(effect.card_token.as_bytes()));
    state.transaction_id = Some(id.clone());
    Control::resume(id)
}
```

## Running and compensating

After running, inspect the result. On cancellation, the accumulated state tells you exactly what needs undoing:

```rust
let mut state = SagaState { fail_at: Some("payment".into()), ..Default::default() };

let result = process_order(order)
    .handle(handle_reserve)
    .handle(handle_payment)
    .handle(handle_confirmation)
    .handle(handle_shipping)
    .run_stateful(&mut state)
    .await;

match result {
    Ok(summary) => println!("SUCCESS: {summary}"),
    Err(_cancelled) => {
        println!("FAILED: {}", state.failure_reason.as_deref().unwrap_or("unknown"));
        state.compensate(); // reverse completed steps in order
    }
}
```

## Testing with mock handlers

Because workflow logic and service implementations are decoupled, tests swap in mock handlers with predictable behavior — no real services needed:

```rust
let result = process_order(test_order())
    .handle(async |_: ReserveInventory| Control::resume("RES-FAKE-0".into()))
    .handle(async |_: ChargePayment|    Control::resume("TXN-FAKE".into()))
    .handle(async |_: SendConfirmation| Control::resume(()))
    .handle(async |_: ShipOrder|        Control::resume("TRK-FAKE".into()))
    .run()
    .await;

assert!(result.unwrap().contains("TRK-FAKE"));
```

To assert on effect payloads, extend the shared state with capture fields and write to them from the handler. See the [full example](https://github.com/romac/corophage/blob/main/corophage/examples/saga.rs) for complete tests covering payment failure, shipping failure, payload assertions, and stateless mock handlers.
