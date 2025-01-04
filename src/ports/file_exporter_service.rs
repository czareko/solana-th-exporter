use std::fs::File;
use std::path::Path;
use std::io::Write;
use crate::domain::TransactionRecord;

pub struct FileExporterService;

impl FileExporterService{

    pub fn save_transactions_to_csv(records: Vec<TransactionRecord>, file_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let path = Path::new(file_name);
        let mut file = File::create(&path)?;

        // Zapisanie nagłówków kolumn
        writeln!(
            file,
            "date,tx_hash,tx_src,tx_dest,sent_amount,sent_currency,received_amount,received_currency,fee_amount,fee_currency"
        )?;

        // Zapisanie danych
        for record in records {
            writeln!(
                file,
                "{},{},{},{},{},{},{},{},{},{}",
                record.date,
                record.tx_hash,
                record.tx_src,
                record.tx_dest,
                record.sent_amount.map_or("N/A".to_string(), |amt| amt.to_string()),
                record.sent_currency.as_deref().unwrap_or("N/A"),
                record.received_amount.map_or("N/A".to_string(), |amt| amt.to_string()),
                record.received_currency.as_deref().unwrap_or("N/A"),
                record.fee_amount,
                record.fee_currency,
            )?;
        }

        log::info!("Transactions successfully saved to {}", file_name);
        Ok(())
    }
}