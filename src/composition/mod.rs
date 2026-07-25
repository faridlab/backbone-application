//! The composition layer — wires the domain modules' event seams into a coherent flow.
//!
//! This is where the bounded contexts MEET: the gateway's `GatewayTransactionSettled` creates a
//! PaymentEntry; payment's `PaymentSettled` draws down billing's invoice outstanding; a refund
//! reverses the whole chain. Each event fires through a sink the composition implements; the sink
//! spawns an async task (fire-and-forget — the events are durable via the outbox) that calls the
//! target module's service.
//!
//! The GL-posting seam (`GlPostSink`) is shared (from `backbone-gl-posting`); production forwards to
//! backbone-accounting's `PostingService`. Here it traces (the skeleton's default — real apps inject
//! the accounting adapter).

use std::sync::Arc;

use uuid::Uuid;

use backbone_payment_gateway::application::service::{
    AccountingPostEnvelope, GlPostAck, GlPostRejected, GlPostSink,
};
use backbone_payment::application::service::payment_events::{
    PaymentCancelled, PaymentEvent, PaymentEventSink, PaymentReceivedOnAccount, PaymentSettled,
};
use backbone_payment::application::service::payment_write_service::{NewPayment, PaymentWriteService};
use backbone_payment_gateway::application::service::gateway_events::{
    GatewayEvent, GatewayEventSink, GatewayTransactionRefunded, GatewayTransactionSettled,
};

/// The shared GL-posting sink. Production: forward to backbone-accounting's PostingService (map
/// AccountingPostEnvelope → PostingRequest, inject cost_center/project/department). Skeleton: trace.
#[derive(Clone)]
pub struct CompositionGlSink;

#[async_trait::async_trait]
impl GlPostSink for CompositionGlSink {
    async fn post(&self, env: &AccountingPostEnvelope) -> Result<GlPostAck, GlPostRejected> {
        let (dr, cr) = env.totals();
        tracing::info!(
            target: "composition.gl",
            source_type = %env.source_type,
            source_id = %env.source_id,
            posting_type = %env.posting_type,
            debit = %dr,
            credit = %cr,
            "GL post (production: forward to backbone-accounting PostingService)"
        );
        Ok(GlPostAck {
            post_id: Uuid::new_v4(),
            journal_id: Uuid::new_v4(),
            idempotent_reuse: false,
        })
    }
}

/// The gateway event consumer. On settle → creates a PaymentEntry (paid = gross). On refund → reverses it.
/// Fire-and-forget: spawns an async task (the event is durable via the outbox; the consumer is at-least-once).
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
                    if let Err(e) = this.on_settled(&s).await {
                        tracing::error!(target: "composition.gateway", error = %e, "failed to create PaymentEntry from gateway settle");
                    }
                }
                GatewayEvent::GatewayTransactionRefunded(s) => {
                    if let Err(e) = this.on_refunded(&s).await {
                        tracing::error!(target: "composition.gateway", error = %e, "failed to reverse PaymentEntry from gateway refund");
                    }
                }
            }
        });
    }
}

impl CompositionGatewaySink {
    /// Gateway settled → create a PaymentEntry at gross + post it. The gateway's fee companion post
    /// already ran (via the GlPostSink); this creates the settlement post (Dr Bank · Cr A/R).
    async fn on_settled(&self, s: &GatewayTransactionSettled) -> Result<(), anyhow::Error> {
        let payment_id = self
            .payments
            .create_payment(NewPayment {
                payment_number: format!("GW-{}", &s.provider_transaction_id[..8.min(s.provider_transaction_id.len())]),
                company_id: s.company_id,
                branch_id: None,
                payment_type: if s.direction == "receive" { "receive".into() } else { "pay".into() },
                party_type: s.party_type.clone(),
                party_id: s.party_id,
                posting_date: s.settled_at.date_naive(),
                currency: Some(s.currency.clone()),
                mode_of_payment_id: None,
                bank_account_id: Uuid::new_v4(),      // production: resolve from the gateway's settlement account
                party_account_id: Uuid::new_v4(),      // production: resolve party → A/R or A/P control
                paid_amount: s.gross_amount,
                reference_no: Some(s.provider_transaction_id.clone()),
                allocations: vec![],
            })
            .await?;
        self.payments.post_payment(payment_id, self.gl.as_ref()).await?;
        tracing::info!(target: "composition.gateway", payment_id = %payment_id, gross = %s.gross_amount, "PaymentEntry created + posted from gateway settle");
        Ok(())
    }

    /// Gateway refunded → reverse the PaymentEntry. The gateway's fee reversal already ran; this
    /// reverses the settlement post (payment.reverse_payment posts the sign-flipped mirror).
    async fn on_refunded(&self, s: &GatewayTransactionRefunded) -> Result<(), anyhow::Error> {
        if let Some(payment_entry_id) = s.payment_entry_id {
            self.payments.reverse_payment(payment_entry_id, self.gl.as_ref()).await?;
            tracing::info!(target: "composition.gateway", payment_id = %payment_entry_id, "PaymentEntry reversed from gateway refund");
        }
        Ok(())
    }
}

/// The payment event consumer. On PaymentSettled → billing drawdown (apply_settlements_once with inbox
/// dedup). On PaymentCancelled → billing reverse_settlement. On PaymentReceivedOnAccount → trace (on-account
/// credit awaiting reconciliation).
#[derive(Clone, Default)]
pub struct CompositionPaymentSink;

impl PaymentEventSink for CompositionPaymentSink {
    fn publish(&self, event: PaymentEvent) {
        match event {
            PaymentEvent::PaymentSettled(s) => {
                tracing::info!(
                    target: "composition.payment",
                    payment_id = %s.payment_id,
                    allocations = s.allocations.len(),
                    "PaymentSettled — production: billing.apply_settlements_once (inbox-deduped)"
                );
            }
            PaymentEvent::PaymentReceivedOnAccount(s) => {
                tracing::info!(
                    target: "composition.payment",
                    payment_id = %s.payment_id,
                    amount = %s.unallocated_amount,
                    "PaymentReceivedOnAccount — on-account credit (awaiting reconciliation)"
                );
            }
            PaymentEvent::PaymentCancelled(s) => {
                tracing::info!(
                    target: "composition.payment",
                    payment_id = %s.payment_id,
                    allocations = s.allocations.len(),
                    "PaymentCancelled — production: billing.reverse_settlement (restore outstanding)"
                );
            }
        }
    }
}
