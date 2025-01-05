use std::str::FromStr;
use chrono::{TimeZone, Utc};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage, UiRawMessage, UiTransactionEncoding, UiTransactionStatusMeta};
use solana_transaction_status::option_serializer::OptionSerializer;
use crate::domain::TransactionRecord;

pub struct SolanaTHService;

impl SolanaTHService {
    pub fn fetch_transactions(pubkey: Pubkey, operation_limit: usize) -> Vec<TransactionRecord> {
        let rpc_url = "https://api.mainnet-beta.solana.com";

        let client = RpcClient::new(rpc_url);

        let confirmed_signatures = client
            .get_signatures_for_address(&pubkey)
            .expect("Failed to fetch signatures");

        let mut records = Vec::new();

        log::debug!("Number of signatures: {}",confirmed_signatures.len().clone());
        let mut index = 0;

        for signature_info in confirmed_signatures.clone() {
            let tx_hash = signature_info.signature.to_string();
            let signature = Signature::from_str(&tx_hash).expect("Invalid signature format");

            let config = RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Json),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            };

            match client.get_transaction_with_config(&signature, config) {
                Ok(transaction) => {
                    match Self::process_transaction(tx_hash, &transaction, &pubkey, &client) {
                        Ok(Some(tx_record)) => {
                            log::debug!("TX: {}", tx_record);
                            records.push(tx_record);
                        }
                        Ok(None) => {
                            log::debug!("TX: Skipping empty result");
                        }
                        Err(err) => {
                            log::error!("Error processing transaction: {:?}", err);
                        }
                    }
                }
                Err(err) => {
                    log::error!("TX download error: {:?}", err);
                }
            }
            index += 1;
            log::info!("Processed: {}/{}",index,confirmed_signatures.len());
            if operation_limit > 0 && index >= operation_limit{
                log::info!("Limit reached - operation processing finished");
                break;
            }
        }

        records
    }

    fn process_transaction(
        tx_hash: String,
        tx: &EncodedConfirmedTransactionWithStatusMeta,
        wallet: &Pubkey,
        client: &RpcClient,
    ) -> std::result::Result<Option<TransactionRecord>, Box<dyn std::error::Error>> {
        log::info!("Process transaction: {}", tx_hash);

        let meta = tx.transaction.meta.as_ref().ok_or("Missing transaction metadata")?;
        let message = match &tx.transaction.transaction {
            EncodedTransaction::Json(raw_transaction) => {
                if let UiMessage::Raw(message) = &raw_transaction.message {
                    message
                } else {
                    return Err("Unsupported message format".into());
                }
            }
            _ => return Err("Unsupported transaction encoding".into()),
        };

        let fee_amount = meta.fee as f64 / 1_000_000_000.0;

        let (sol_change, token_change,token_mint) = Self::calculate_balance_changes(meta, wallet, message);

        let transaction_type = Self::classify_transaction_type(
            Some(sol_change).filter(|&x| x.abs() > 0.0),
            Some(token_change).filter(|&x| x.abs() > 0.0),
        );

        log::info!(
        "Detected transaction: Type: {}, SOL Change: {:?}, Token Change: {:?}",
        transaction_type,
        sol_change,
        token_change
    );

        let transaction = TransactionRecord {
            date: Self::format_date(tx.block_time.unwrap_or(0) as u64),
            tx_hash,
            tx_src: message.account_keys.get(0).cloned().unwrap_or_default(),
            tx_dest: message.account_keys.get(1).cloned().unwrap_or_default(),
            sent_amount: if sol_change < 0.0 || token_change < 0.0 {
                Some(sol_change.min(token_change).abs())
            } else {
                None
            },
            sent_currency: if sol_change < 0.0 {
                Some("SOL".to_string())
            } else if token_change < 0.0 {
                token_mint.as_ref().and_then(|mint| Self::get_token_symbol(client, mint))
            } else {
                None
            },
            received_amount: if sol_change > 0.0 || token_change > 0.0 {
                Some(sol_change.max(token_change))
            } else {
                None
            },
            received_currency: if sol_change > 0.0 {
                Some("SOL".to_string())
            } else if token_change > 0.0 {
                token_mint.as_ref().and_then(|mint| Self::get_token_symbol(client, mint))
            } else {
                None
            },
            fee_amount,
            fee_currency: "SOL".to_string(),
        };

        log::info!("Transaction Record: {}", transaction);

        Ok(Some(transaction))
    }

    fn calculate_balance_changes(
        meta: &UiTransactionStatusMeta,
        wallet: &Pubkey,
        message: &UiRawMessage,
    ) -> (f64, f64, Option<String>) {
        let mut sol_change = 0.0;
        let mut token_change = 0.0;
        let mut token_mint = None;

        if let Some(pre_balance) = meta.pre_balances.iter().enumerate().find_map(|(index, &pre)| {
            message.account_keys.get(index).and_then(|key| {
                if key == &wallet.to_string() {
                    Some(pre)
                } else {
                    None
                }
            })
        }) {
            if let Some(post_balance) = meta.post_balances.iter().enumerate().find_map(|(index, &post)| {
                message.account_keys.get(index).and_then(|key| {
                    if key == &wallet.to_string() {
                        Some(post)
                    } else {
                        None
                    }
                })
            }) {
                sol_change = (post_balance as f64 - pre_balance as f64) / 1_000_000_000.0;
            }
        }

        // Oblicz zmiany w tokenach
        if let (OptionSerializer::Some(pre_token_balances), OptionSerializer::Some(post_token_balances)) =
            (&meta.pre_token_balances, &meta.post_token_balances)
        {
            for (pre_balance, post_balance) in pre_token_balances.iter().zip(post_token_balances.iter())
            {
                if let Some(_account_index) = message.account_keys.iter().position(|key| {
                    key == &wallet.to_string()
                        && pre_balance.owner == OptionSerializer::Some(wallet.to_string())
                        && pre_balance.mint == post_balance.mint
                }) {
                    let pre_amount = pre_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                    let post_amount = post_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                    let difference = post_amount - pre_amount;
                    token_change += difference;

                    if difference != 0.0 {
                        token_mint = Some(pre_balance.mint.clone());
                    }
                }
            }
        }

        // Odejmij opłatę od zmiany SOL
        let fee = meta.fee as f64 / 1_000_000_000.0;
        sol_change -= fee;

        (sol_change, token_change, token_mint)
    }

    fn classify_transaction_type(
        sol_difference: Option<f64>,
        token_difference: Option<f64>,
    ) -> String {
        match (sol_difference, token_difference) {
            (Some(sol), Some(token)) if sol > 0.0 && token < 0.0 => "Token Swap".to_string(),
            (Some(sol), None) if sol > 0.0 => "SOL Deposit".to_string(),
            (Some(sol), None) if sol < 0.0 => "SOL Withdrawal".to_string(),
            (None, Some(token)) if token > 0.0 => "Token Deposit".to_string(),
            (None, Some(token)) if token < 0.0 => "Token Withdrawal".to_string(),
            (Some(sol), Some(token)) if sol < 0.0 && token > 0.0 => "Token Purchase".to_string(),
            _ => "Unknown".to_string(),
        }
    }


    fn format_date(timestamp: u64) -> String {
        match Utc.timestamp_opt(timestamp as i64, 0) {
            chrono::LocalResult::Single(datetime) => datetime.format("%Y-%m-%d %H:%M:%S").to_string(),
            _ => "1970-01-01 00:00:00".to_string(),
        }
    }

    fn get_token_symbol(client: &RpcClient, mint_address: &str) -> Option<String> {
        let metadata_program_id = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

        let program_id = Pubkey::from_str(metadata_program_id).ok()?;
        let mint = Pubkey::from_str(mint_address).ok()?;

        let seeds = &[
            b"metadata".as_ref(),
            program_id.as_ref(),
            mint.as_ref(),
        ];

        let (metadata_pda, _) = Pubkey::find_program_address(seeds, &program_id);

        if let Ok(account) = client.get_account(&metadata_pda) {
            if let Ok(metadata) =
                mpl_token_metadata::accounts::Metadata::safe_deserialize(&account.data)
            {
                return Some(metadata.symbol.trim().to_string());
            }
        }

        None
    }
}