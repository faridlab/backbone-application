//! Concrete payment-gateway providers — plugged in at the composition layer (ADR-001 §2).
//! The gateway MODULE ships only the trait + stubs; real HTTP clients live HERE.

use rust_decimal::Decimal;

use backbone_payment_gateway::application::service::gateway_provider::{
    ChargeCreated, CreateChargeRequest, GatewayError as ProviderError, GatewayTxStatus,
    PaymentGatewayProvider, RefundResult,
};
use backbone_payment_gateway::GatewayTransactionStatus;

/// A Midtrans (snap + core API) provider. Works against the sandbox when `MIDTRANS_SERVER_KEY` is set;
/// without credentials the calls fail with a clear auth error (the provider is structurally complete —
/// live calls require the key).
pub struct MidtransProvider {
    client: reqwest::Client,
    server_key: String,
    base_url: String,
}

impl MidtransProvider {
    /// Create from env: `MIDTRANS_SERVER_KEY` + `MIDTRANS_SANDBOX=true|false`.
    pub fn from_env() -> Self {
        let server_key = std::env::var("MIDTRANS_SERVER_KEY")
            .unwrap_or_else(|_| "TEST-KEY-NOT-SET".into());
        let sandbox = std::env::var("MIDTRANS_SANDBOX")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);
        Self::new(&server_key, sandbox)
    }

    pub fn new(server_key: &str, sandbox: bool) -> Self {
        Self {
            client: reqwest::Client::new(),
            server_key: server_key.into(),
            base_url: if sandbox {
                "https://app.sandbox.midtrans.com".into()
            } else {
                "https://app.midtrans.com".into()
            },
        }
    }

    /// Midtrans uses the Server Key as Basic Auth username (empty password).
    fn snap_url(&self, path: &str) -> String {
        format!("{}/snap/v1{}", self.base_url, path)
    }
    fn core_url(&self, path: &str) -> String {
        format!("{}/v2{}", self.base_url, path)
    }
}

#[async_trait::async_trait]
impl PaymentGatewayProvider for MidtransProvider {
    async fn create_charge(&self, req: &CreateChargeRequest) -> Result<ChargeCreated, ProviderError> {
        let order_id = format!("{}-{}", req.reference, uuid::Uuid::new_v4().simple());
        let body = serde_json::json!({
            "transaction_details": {
                "order_id": &order_id,
                "gross_amount": req.amount,
            },
            "currency": req.currency,
        });
        let resp = self
            .client
            .post(self.snap_url("/transactions"))
            .basic_auth(&self.server_key, Some(""))
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            let code = resp.status().as_u16().to_string();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Provider { code, message: msg });
        }
        let json: serde_json::Value = resp.json().await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;
        let redirect_url = json.get("redirect_url").and_then(|v| v.as_str()).map(String::from);
        Ok(ChargeCreated {
            provider_transaction_id: order_id,
            status: GatewayTransactionStatus::Pending,
            redirect_url,
        })
    }

    async fn get_status(&self, provider_tx_id: &str) -> Result<GatewayTxStatus, ProviderError> {
        let resp = self
            .client
            .get(self.core_url(&format!("/{}/status", provider_tx_id)))
            .basic_auth(&self.server_key, Some(""))
            .send()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ProviderError::NotFound(provider_tx_id.into()));
        }
        let json: serde_json::Value = resp.json().await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;
        let status_str = json.get("transaction_status").and_then(|v| v.as_str()).unwrap_or("pending");
        let gross = json.get("gross_amount").and_then(|v| v.as_str())
            .and_then(|s| Decimal::from_str_exact(s).ok())
            .unwrap_or_default();
        // Map Midtrans statuses to our lifecycle.
        let status = match status_str {
            "settlement" | "capture" => GatewayTransactionStatus::Settled,
            "authorize" => GatewayTransactionStatus::Authorized,
            "refund" => GatewayTransactionStatus::Refunded,
            "deny" | "expire" | "cancel" | "failure" => GatewayTransactionStatus::Failed,
            _ => GatewayTransactionStatus::Pending,
        };
        let fee = json.get("transaction_time") // Midtrans doesn't report fee in status; fee comes from settlement notification
            .map(|_| Decimal::ZERO)
            .unwrap_or_default();
        Ok(GatewayTxStatus {
            status,
            gross_amount: gross,
            fee_amount: Some(fee),
            net_amount: if fee > Decimal::ZERO { Some(gross - fee) } else { None },
            settled_at: None, // production: parse settlement_time from the response
        })
    }

    async fn refund(&self, provider_tx_id: &str, amount: Decimal) -> Result<RefundResult, ProviderError> {
        let body = serde_json::json!({ "amount": amount.to_string(), "reason": "gateway reversal" });
        let resp = self
            .client
            .post(self.core_url(&format!("/{}/refund", provider_tx_id)))
            .basic_auth(&self.server_key, Some(""))
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            let code = resp.status().as_u16().to_string();
            let msg = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Provider { code, message: msg });
        }
        Ok(RefundResult {
            provider_transaction_id: provider_tx_id.into(),
            refunded_amount: amount,
            status: GatewayTransactionStatus::Refunded,
        })
    }

    fn name(&self) -> &'static str {
        "midtrans"
    }
}
