# Composition Recipes

Proven patterns for composing domain modules into a Backbone service. Copy the parts you need when
your app adds modules via `metaphor add module`. The skeleton stays generic; these recipes are the
reference.

- **Recipe A — payment + payment-gateway + billing** (next): the event-seam ACLs for the payment stack.
- **Recipe B — billing → tax audit mirror** (end of file): drain billing's outbox into tax's `TaxTransaction` + e-Faktur.

## Recipe A — payment + payment-gateway + billing

### Step 1 — add the domain module deps

```toml
# Cargo.toml [dependencies] — add after the framework crates.
backbone-payment         = { path = "../backbone-payment" }
backbone-payment-gateway = { path = "../backbone-payment-gateway" }
backbone-billing         = { path = "../backbone-billing" }
backbone-accounting      = { path = "../backbone-accounting" }
rust_decimal             = { version = "1.36", features = ["serde"] }
```

### Step 2 — create `src/composition/mod.rs` (the event-seam ACLs)

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

### Step 3 — create `src/composition/providers.rs` (Midtrans sandbox provider)

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

### Step 4 — wire `main.rs`

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

---

## Recipe B — billing → tax audit mirror

The tax module records a `TaxTransaction` (+ a gapless e-Faktur number for sales) for every posted
billing invoice, and voids it on cancellation. **Tax does not self-populate** — billing stages
`SalesInvoicePosted` / `PurchaseInvoicePosted` / `InvoiceCancelled` into `billing.outbox_events`, and
the host app drains that outbox into tax via the `backbone-billing-tax` dispatcher. Copy this recipe
when your app uses the tax module alongside billing.

> **Why a separate `backbone-billing-tax` crate** (not code inside `backbone-billing` or `backbone-tax`)?
> The routing needs types from *both* modules (billing's events + tax's `record_tax_transaction`), so
> whoever hosts it depends on both. A backbone module keeps zero cargo edges to siblings, so the bridge
> can't live in either module — it's a composition-layer concern (type `crate`, not `module`). This is
> the only place the two bounded contexts meet.

### Step 1 — add the deps

```toml
# Cargo.toml [dependencies]
backbone-tax         = { path = "../backbone-tax" }
backbone-billing     = { path = "../backbone-billing" }       # the producer (stages outbox events)
backbone-billing-tax = { path = "../backbone-billing-tax" }   # the dispatcher (drains outbox → tax)
```

### Step 2 — wire `main.rs` (after the DB + the outbox relay block)

```rust
// Tax audit mirror: drain billing.outbox_events → tax.record_tax_transaction / void_for_invoice.
let tax = backbone_tax::TaxModule::builder()
    .with_database(database.pool().clone())
    .build()?;
backbone_outbox::outbox::migrate(database.pool(), "billing").await?;
let dispatcher_pool = database.pool().clone();
let dispatcher_efaktur = tax.efaktur_service.clone();
tokio::spawn(backbone_billing_tax::run_dispatcher(
    dispatcher_pool,
    "billing",
    dispatcher_efaktur,
    async { let _ = tokio::signal::ctrl_c().await; },
));
```

### ⚠️ The double-drain guard

`backbone-application` already runs an outbox relay that drains `database.outbox_schemas` onto its
integration bus. **`"billing"` must NOT be in `outbox_schemas`** — that schema is drained by the
dispatcher above. If both drain `billing.outbox_events`, they race on the same rows. Keep the bus relay
for the other schemas; the dispatcher owns billing.

### What it does (so you know what to expect)

- `SalesInvoicePosted` → records a `TaxTransaction` + assigns a gapless `010.NNN-NN.YYYYYYYY` e-Faktur.
- `PurchaseInvoicePosted` → records a `TaxTransaction` (no e-Faktur; only sales are numbered).
- `InvoiceCancelled` (credit note) → flips the e-Faktur to **Voided** (DJP sequence preserved, never reused).
- All paths idempotent — the outbox delivers at-least-once; tax's unique `(company, invoice_ref, kind)`
  fence makes a redelivery a no-op.

### The producer side — billing must stage events

The dispatcher only drains what billing stages. Construct the billing writer with the outbox schema, or
its events never reach tax:

```rust
let billing = backbone_billing::application::service::billing_write_service::BillingWriteService
    ::new(pool.clone()).with_outbox_schema("billing");
```

### Test it

```bash
cargo test -p backbone-billing-tax --test dispatcher_seam
# outbox_drain_routes_posted_invoice_to_tax + outbox_drain_voids_efaktur_on_invoice_cancelled
```

Requires `DATABASE_URL` (:5433) with `tax` + `billing` schemas migrated.

