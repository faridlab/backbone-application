# Composition Recipe: payment + payment-gateway + billing

This is the proven pattern for composing the payment stack into a Backbone service. Copy the parts
you need when your app adds payment modules via `metaphor add module`. The skeleton stays generic;
this recipe is the reference.

## Step 1 — add the domain module deps

```toml
# Cargo.toml [dependencies] — add after the framework crates.
backbone-payment         = { path = "../backbone-payment" }
backbone-payment-gateway = { path = "../backbone-payment-gateway" }
backbone-billing         = { path = "../backbone-billing" }
backbone-accounting      = { path = "../backbone-accounting" }
rust_decimal             = { version = "1.36", features = ["serde"] }
```

## Step 2 — create `src/composition/mod.rs` (the event-seam ACLs)

The gateway's settle creates a PaymentEntry; payment's settle draws down billing; a refund reverses
the chain. Each event fires through a sink the composition implements.

```rust
use std::sync::Arc;
use uuid::Uuid;
use backbone_payment::application::service::payment_events::*;
use backbone_payment::application::service::payment_write_service::*;
use backbone_payment_gateway::application::service::gateway_events::*;
use backbone_payment_gateway::application::service::{AccountingPostEnvelope, GlPostAck, GlPostRejected, GlPostSink};

pub mod providers;

/// GL-posting seam — production: forward to backbone-accounting's PostingService.
#[derive(Clone)]
pub struct CompositionGlSink;
#[async_trait::async_trait]
impl GlPostSink for CompositionGlSink {
    async fn post(&self, env: &AccountingPostEnvelope) -> Result<GlPostAck, GlPostRejected> {
        tracing::info!(source_type = %env.source_type, "GL post");
        Ok(GlPostAck { post_id: Uuid::new_v4(), journal_id: Uuid::new_v4(), idempotent_reuse: false })
    }
}

/// Gateway event consumer — settle → create PaymentEntry; refund → reverse.
#[derive(Clone)]
pub struct CompositionGatewaySink {
    pub payments: Arc<PaymentWriteService>,
    pub gl: Arc<CompositionGlSink>,
}
impl GatewayEventSink for CompositionGatewaySink {
    fn publish(&self, event: GatewayEvent) {
        let this = self.clone();
        tokio::spawn(async move {
            match event {
                GatewayEvent::GatewayTransactionSettled(s) => {
                    let id = this.payments.create_payment(NewPayment {
                        payment_number: format!("GW-{}", &s.provider_transaction_id[..8]),
                        company_id: s.company_id, branch_id: None,
                        payment_type: "receive".into(),
                        party_type: s.party_type.clone(), party_id: s.party_id,
                        posting_date: s.settled_at.date_naive(),
                        currency: Some(s.currency.clone()),
                        mode_of_payment_id: None,
                        bank_account_id: Uuid::new_v4(),       // resolve from gateway config
                        party_account_id: Uuid::new_v4(),       // resolve party → A/R
                        paid_amount: s.gross_amount,
                        reference_no: Some(s.provider_transaction_id.clone()),
                        allocations: vec![],
                    }).await;
                    if let Ok(pid) = id { let _ = this.payments.post_payment(pid, this.gl.as_ref()).await; }
                }
                GatewayEvent::GatewayTransactionRefunded(s) => {
                    if let Some(pid) = s.payment_entry_id {
                        let _ = this.payments.reverse_payment(pid, this.gl.as_ref()).await;
                    }
                }
            }
        });
    }
}

/// Payment event consumer — settle → billing drawdown; cancel → billing reverse.
#[derive(Clone, Default)]
pub struct CompositionPaymentSink;
impl PaymentEventSink for CompositionPaymentSink {
    fn publish(&self, event: PaymentEvent) {
        // production: billing.apply_settlements_once / reverse_settlement
        tracing::info!(?event, "payment event");
    }
}
```

## Step 3 — create `src/composition/providers.rs` (Midtrans sandbox provider)

```rust
use backbone_payment_gateway::application::service::gateway_provider::*;
use backbone_payment_gateway::GatewayTransactionStatus;

pub struct MidtransProvider { client: reqwest::Client, server_key: String, base_url: String }
impl MidtransProvider {
    pub fn from_env() -> Self { /* read MIDTRANS_SERVER_KEY + MIDTRANS_SANDBOX */ }
}
#[async_trait::async_trait]
impl PaymentGatewayProvider for MidtransProvider {
    // create_charge → POST Snap API; get_status → GET Core API; refund → POST Core API.
}
```

## Step 4 — wire `main.rs`

After the database + migrations + outbox relay:

```rust
mod composition;
use composition::{CompositionGatewaySink, CompositionGlSink, CompositionPaymentSink};

// Build the composition sinks + services.
let gl = Arc::new(CompositionGlSink);
let payments = Arc::new(PaymentWriteService::with_sink(pool, Arc::new(CompositionPaymentSink)));
let gateway_sink = Arc::new(CompositionGatewaySink { payments: payments.clone(), gl: gl.clone() });
let _gateway = GatewayWriteService::with_sink(pool, gateway_sink);

// Merge module routers at the composition point.
let app = Router::new()
    .merge(health_routes(...))
    .merge(PaymentModule::builder().with_database(pool).build()?.all_crud_routes())
    .merge(PaymentGatewayModule::builder().with_database(pool).build()?.all_crud_routes());
```
