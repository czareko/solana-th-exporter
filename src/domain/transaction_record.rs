use std::fmt;
use serde::Serialize;

#[derive(Serialize)]
pub struct TransactionRecord {
    pub date: String,
    pub tx_hash: String,
    pub tx_src: String,
    pub tx_dest: String,
    pub sent_amount: Option<f64>,
    pub sent_currency: Option<String>,
    pub received_amount: Option<f64>,
    pub received_currency: Option<String>,
    pub fee_amount: f64,
    pub fee_currency: String,
}

impl fmt::Display for TransactionRecord {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        log::info!("Transaction Record:");
        log::info!("  Date: {}", self.date);
        log::info!("  Tx Hash: {}", self.tx_hash);
        log::info!("  Source: {}", self.tx_src);
        log::info!("  Destination: {}", self.tx_dest);
        log::info!(
            "  Sent Amount: {} {}",
            self.sent_amount.map_or("N/A".to_string(), |amt| amt.to_string()),
            self.sent_currency.as_deref().unwrap_or("N/A")
        );
        log::info!(
            "  Received Amount: {} {}",
            self.received_amount.map_or("N/A".to_string(), |amt| amt.to_string()),
            self.received_currency.as_deref().unwrap_or("N/A")
        );
        log::info!(
            "  Fee: {} {}",
            self.fee_amount.to_string(),
            self.fee_currency
        );
        Ok(())
    }
}