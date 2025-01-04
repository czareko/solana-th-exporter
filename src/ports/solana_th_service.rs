use std::str::FromStr;
use chrono::{TimeZone, Utc};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::account_info::AccountInfo;
use solana_sdk::clock::Epoch;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage, UiRawMessage, UiTransactionEncoding};
use solana_transaction_status::option_serializer::OptionSerializer;
use spl_token::solana_program::program_pack::Pack;
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
        client: &RpcClient
    ) -> std::result::Result<Option<TransactionRecord>, Box<dyn std::error::Error>> {
        log::info!("Process transaction: {}", tx_hash);

        let block_time = tx.block_time.unwrap_or(0);
        let date = Utc.timestamp_opt(block_time as i64, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let meta = tx.transaction.meta.as_ref().ok_or("Missing transaction metadata")?;
        let fee_amount = meta.fee as f64 / 1_000_000_000.0;

        let raw_message = match &tx.transaction.transaction {
            EncodedTransaction::Json(raw_transaction) => &raw_transaction.message,
            _ => return Err("Unsupported transaction encoding".into()),
        };

        let message = match raw_message {
            UiMessage::Raw(message) => Some(message),
            _ => None,
        }.unwrap();

        let tx_src = message
            .account_keys
            .get(0)
            .map(|account| account.to_string())
            .unwrap_or_else(|| "n/a".to_string());

        let tx_dest = message
            .account_keys
            .get(1)
            .map(|account| account.to_string())
            .unwrap_or_else(|| "n/a".to_string());

        let system_program_id = "11111111111111111111111111111111";
        let token_program_id = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

        let mut total_sent_amount: f64 = 0.0;
        let mut total_received_amount: f64 = 0.0;
        let mut sent_currency: Option<String> = None;
        let mut received_currency: Option<String> = None;

        for instruction in message.instructions.clone() {
            if let Some(program_id) = message.account_keys.get(instruction.program_id_index as usize) {
                if program_id == &system_program_id {
                    // SOL transfer
                    if let (Some(source_index), Some(dest_index)) =
                        (instruction.accounts.get(0), instruction.accounts.get(1))
                    {
                        let source = message.account_keys.get(*source_index as usize);
                        let dest = message.account_keys.get(*dest_index as usize);

                        if source == Some(&wallet.to_string()) {
                            if let Some(amount) = meta.pre_balances.get(*source_index as usize) {
                                total_sent_amount = *amount as f64 / 1_000_000_000.0;
                            }
                            sent_currency = Some("SOL".to_string());
                        }

                        if dest == Some(&wallet.to_string()) {
                            if let Some(amount) = meta.post_balances.get(*dest_index as usize) {
                                total_received_amount = *amount as f64 / 1_000_000_000.0;
                            }
                            received_currency = Some("SOL".to_string());
                        }
                    }
                } else if program_id == &token_program_id {
                    // SPL Token transfer
                    if let (Some(source_index), Some(dest_index)) =
                        (instruction.accounts.get(0), instruction.accounts.get(1))
                    {
                        let source = message.account_keys.get(*source_index as usize);
                        let dest = message.account_keys.get(*dest_index as usize);

                        if source == Some(&wallet.to_string()) {
                            match &meta.pre_token_balances {
                                OptionSerializer::Some(pre_balances) => {
                                    for balance in pre_balances {
                                        if let Some(key) = message.account_keys.get(balance.account_index as usize) {
                                            if let Some(src_key) = message.account_keys.get(*source_index as usize) {
                                                if key == src_key {
                                                    if let Some(amount) = balance.ui_token_amount.ui_amount {
                                                        total_sent_amount = amount;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                OptionSerializer::None => {},
                                OptionSerializer::Skip => {}
                            }
                            if total_sent_amount > 0.0 {
                                sent_currency = Some(Self::decode_currency(*source_index as usize, &message, &client));
                            }

                        }

                        if dest == Some(&wallet.to_string()) {
                            match &meta.post_token_balances {
                                OptionSerializer::Some(post_balances) => {
                                    for balance in post_balances {
                                        if let Some(key) = message.account_keys.get(balance.account_index as usize) {
                                            if let Some(dst_key) = message.account_keys.get(*dest_index as usize) {
                                                if key == dst_key {
                                                    if let Some(amount) = balance.ui_token_amount.ui_amount {
                                                        total_received_amount = amount;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                OptionSerializer::None => {},
                                OptionSerializer::Skip => {}
                            }
                            if total_received_amount > 0.0{
                                received_currency = Some(Self::decode_currency(*dest_index as usize, &message, &client));
                            }
                        }
                    }
                }
            }
        }

        if total_sent_amount > 0.0 && total_received_amount > 0.0 {
            log::info!(
            "TRADE: Total Sent: {:.9} {}, Total Received: {:.9} {}",
            total_sent_amount,
            sent_currency.clone().unwrap_or_else(|| "Unknown".to_string()),
            total_received_amount,
            received_currency.clone().unwrap_or_else(|| "Unknown".to_string())
        );
        } else if total_received_amount > 0.0 {
            log::info!(
            "DEPOSIT: Total Received: {:.9} {}",
            total_received_amount,
            received_currency.clone().unwrap_or_else(|| "Unknown".to_string())
        );
        } else if total_sent_amount > 0.0 {
            log::info!(
            "WITHDRAWAL: Total Sent: {:.9} {}",
            total_sent_amount,
            sent_currency.clone().unwrap_or_else(|| "Unknown".to_string())
        );
        } else {
            log::info!("ELSE: No relevant data found.");
            return Ok(None);
        }


        // Initialize transaction record
        let transaction = TransactionRecord {
            date,
            tx_hash,
            tx_src,
            tx_dest,
            sent_amount: Some(total_sent_amount),
            sent_currency,
            received_amount: Some(total_received_amount),
            received_currency,
            fee_amount,
            fee_currency: "SOL".to_string(),
        };

        Ok(Some(transaction))
    }

    fn decode_currency(
        account_index: usize,
        message: &UiRawMessage,
        rpc_client: &RpcClient,
    ) -> String {
        let account_key = match message.account_keys.get(account_index) {
            Some(key) => key,
            None => return "Unknown".to_string()
        };

        if let Ok(account) = rpc_client.get_account(&Pubkey::from_str(account_key).unwrap()) {
            if let Ok(token_account) = spl_token::state::Account::unpack(&account.data) {
                return Self::get_token_symbol(rpc_client, &token_account.mint.to_string())
                    .unwrap_or_else(|| "Unknown SPL Token".to_string());
            }
        }

        "Unknown".to_string()
    }

    fn get_token_symbol(client: &RpcClient, mint_address: &str) -> Option<String> {
        let metadata_program_id = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

        let program_id = Pubkey::from_str(metadata_program_id).ok()?;
        let mint = Pubkey::from_str(mint_address).ok()?;

        let seeds = &[
            b"metadata".as_ref(),
            program_id.as_ref(),
            mint.as_ref()
        ];

        let (metadata_pda, _) = Pubkey::find_program_address(seeds, &program_id);

        let mut metadata_account = client.get_account(&metadata_pda).ok()?;
        let mut lamports = metadata_account.lamports;
        let account_info = AccountInfo::new(
            &metadata_pda,
            false,
            false,
            &mut lamports,
            &mut metadata_account.data[..],
            &program_id,
            false,
            Epoch::default(),
        );

        let metadata = mpl_token_metadata::accounts::Metadata::try_from(&account_info).ok()?;
        Some(metadata.symbol)
    }

}