//! Real `GlPostSink` for backbone-asset: forwards the lifecycle's balanced postings to
//! backbone-accounting's `PostingService`.
//!
//! Promoted from the in-test ACL adapter at `backbone-asset/tests/common/mod.rs` (GlAdapter) — the
//! asset module ships with ZERO Cargo edge to accounting, so the envelope→PostingRequest mapping
//! lives here in the composition layer, the only place the two bounded contexts meet.
//!
//! Idempotency: the asset engine emits a deterministic `source_id` per verb (acquire/dispose derive
//! from the asset id; each depreciation period from its schedule entry id), and accounting dedupes by
//! `(source_type, source_id)` — so a retry re-posts the same source and is deduped, never double-posted.

use std::sync::Arc;

use backbone_accounting::application::service::posting_service::{
    PostingLine, PostingRequest, PostingService,
};
use backbone_accounting::infrastructure::persistence::SqlxPostingRepository;
use backbone_asset::application::service::{
    AccountingPostEnvelope, GlPostAck, GlPostRejected, GlPostSink,
};
use sqlx::PgPool;

/// A `GlPostSink` backed by the real ledger. Clone-cheap via the inner `Arc`.
#[derive(Clone)]
pub struct AssetAccountingGlSink {
    pub posting: Arc<PostingService>,
}

impl AssetAccountingGlSink {
    pub fn new(pool: PgPool) -> Self {
        Self {
            posting: Arc::new(PostingService::new(Arc::new(SqlxPostingRepository::new(pool)))),
        }
    }
}

#[async_trait::async_trait]
impl GlPostSink for AssetAccountingGlSink {
    async fn post(&self, env: &AccountingPostEnvelope) -> Result<GlPostAck, GlPostRejected> {
        let mut req =
            PostingRequest::original(env.company_id, &env.source_type, env.source_id, env.posting_date);
        req.source_reference = env.source_reference.clone();
        req.posting_type = env.posting_type.clone();
        req.lines = env
            .lines
            .iter()
            .map(|l| PostingLine {
                account_id: l.account_id,
                debit: l.debit,
                credit: l.credit,
                party_type: l.party_type.clone(),
                party_id: l.party_id,
                cost_center_id: None,
                project_id: None,
                department_id: None,
                description: l.description.clone(),
            })
            .collect();

        match self.posting.post(req, None).await {
            Ok(x) => Ok(GlPostAck {
                post_id: x.post_id,
                journal_id: x.journal_id,
                idempotent_reuse: x.idempotent_reuse,
            }),
            Err(x) => Err(GlPostRejected { code: x.code().to_string(), message: x.to_string() }),
        }
    }
}
