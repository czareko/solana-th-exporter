use std::str::FromStr;
use chrono::{TimeZone, Utc};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::account_info::AccountInfo;
use solana_sdk::clock::Epoch;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiCompiledInstruction, UiInstruction, UiMessage, UiRawMessage, UiTransactionEncoding, UiTransactionStatusMeta};
use solana_transaction_status::option_serializer::OptionSerializer;
use spl_token::instruction::TokenInstruction;
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
                    match Self::process_transaction_3(tx_hash, &transaction, &pubkey, &client) {
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

    fn get_account_index_from_instruction(
        instruction: &UiCompiledInstruction,
        message: &UiRawMessage,
        wallet: &Pubkey,
    ) -> Option<usize> {
        for &account_index in &instruction.accounts {
            if let Some(account_key) = message.account_keys.get(account_index as usize) {
                if account_key.to_string() == wallet.to_string() {
                    return Some(account_index as usize);
                }
            }
        }
        None
    }

    fn process_transaction_2(tx_hash: String,
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

        //Self::debug_token_balances(meta);

        for instruction in &message.instructions {
            if let Some(account_index) = Self::get_account_index_from_instruction(instruction, message, wallet) {
                            let (sol_change, token_change) = Self::detect_balance_changes(meta, account_index);
                            let transaction_type = Self::classify_transaction_type(sol_change, token_change);

                            log::info!(
                    "Detected transaction: Type: {}, SOL Change: {:?}, Token Change: {:?}",
                    transaction_type,
                    sol_change,
                    token_change
                );
            }
        }

        // Obsługa instrukcji
        let mut total_sent_amount = 0.0;
        let mut total_received_amount = 0.0;
        let mut sent_currency = None;
        let mut received_currency = None;

        Self::process_compiled_instructions(
            &message.instructions,
            &message,
            &meta,
            wallet,
            &client,
            &mut total_sent_amount,
            &mut total_received_amount,
            &mut sent_currency,
            &mut received_currency,
        );

        log::info!("--- TSA: {}, TRA: {}",total_sent_amount, total_received_amount);

        if let OptionSerializer::Some(inner_instructions) = &meta.inner_instructions {
            for inner in inner_instructions {
                Self::process_inner_instructions(
                    &inner.instructions,
                    &message,
                    &meta,
                    wallet,
                    &client,
                    &mut total_sent_amount,
                    &mut total_received_amount,
                    &mut sent_currency,
                    &mut received_currency,
                );
            }
        }

        log::info!("------ TSA: {}, TRA: {}",total_sent_amount, total_received_amount);

        Self::classify_transaction(
            total_sent_amount,
            total_received_amount,
            sent_currency.clone(),
            received_currency.clone(),
        );

        let transaction = TransactionRecord {
            date: Self::format_date(tx.block_time.unwrap_or(0) as u64),
            tx_hash,
            tx_src: message.account_keys.get(0).cloned().unwrap_or_default(),
            tx_dest: message.account_keys.get(1).cloned().unwrap_or_default(),
            sent_amount: Some(total_sent_amount),
            sent_currency,
            received_amount: Some(total_received_amount),
            received_currency,
            fee_amount,
            fee_currency: "SOL".to_string(),
        };

        log::info!("Transaction: {}", transaction);

        Ok(Some(transaction))
    }

    fn process_transaction_3(
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

        // Opłata transakcyjna
        let fee_amount = meta.fee as f64 / 1_000_000_000.0;

        // Oblicz zmiany salda
        let (sol_change, token_change,token_mint) = Self::calculate_balance_changes(meta, wallet, message, client);

        // Klasyfikacja typu transakcji
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

        // Tworzenie rekordu transakcji
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
                token_mint.as_ref().and_then(|mint| Self::get_token_symbol_2(client, mint))
                //Some("TOKEN".to_string()) // Możesz rozwinąć logikę do rozpoznawania tokenu
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
                token_mint.as_ref().and_then(|mint| Self::get_token_symbol_2(client, mint))
            } else {
                None
            },
            fee_amount,
            fee_currency: "SOL".to_string(),
        };

        log::info!("Transaction Record: {}", transaction);

        Ok(Some(transaction))
    }

    fn extract_token_mint(
        meta: &UiTransactionStatusMeta,
        wallet: &Pubkey,
        account_index: usize,
    ) -> Option<String> {
        // Sprawdź w `pre_token_balances`
        if let OptionSerializer::Some(pre_balances) = &meta.pre_token_balances {
            if let Some(balance) = pre_balances.iter().find(|balance| {
                balance.account_index as usize == account_index
                    && balance.owner.as_ref().map(|o| o == &wallet.to_string()) == Some(true)
            }) {
                return Some(balance.mint.clone());
            }
        }

        // Sprawdź w `post_token_balances`
        if let OptionSerializer::Some(post_balances) = &meta.post_token_balances {
            if let Some(balance) = post_balances.iter().find(|balance| {
                balance.account_index as usize == account_index
                    && balance.owner.as_ref().map(|o| o == &wallet.to_string()) == Some(true)
            }) {
                return Some(balance.mint.clone());
            }
        }

        None
    }

    fn calculate_balance_changes(
        meta: &UiTransactionStatusMeta,
        wallet: &Pubkey,
        message: &UiRawMessage,
        client: &RpcClient,
    ) -> (f64, f64, Option<String>) {
        let mut sol_change = 0.0;
        let mut token_change = 0.0;
        let mut token_mint = None;

        // Oblicz zmiany w SOL
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
                // Uwzględnij opłatę w różnicy SOL
                sol_change = (post_balance as f64 - pre_balance as f64) / 1_000_000_000.0;
            }
        }

        // Oblicz zmiany w tokenach
        if let (OptionSerializer::Some(pre_token_balances), OptionSerializer::Some(post_token_balances)) =
            (&meta.pre_token_balances, &meta.post_token_balances)
        {
            for (pre_balance, post_balance) in pre_token_balances.iter().zip(post_token_balances.iter())
            {
                if let Some(account_index) = message.account_keys.iter().position(|key| {
                    key == &wallet.to_string()
                        && pre_balance.owner == OptionSerializer::Some(wallet.to_string())
                        && pre_balance.mint == post_balance.mint
                }) {
                    let pre_amount = pre_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                    let post_amount = post_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                    let difference = post_amount - pre_amount;
                    token_change += difference;

                    // Zapisz `mint`, jeśli znaleziono różnicę
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

    fn debug_token_balances(meta: &UiTransactionStatusMeta) {
        if let OptionSerializer::Some(pre_token_balances) = &meta.pre_token_balances {
            log::info!("Pre Token Balances:");
            for balance in pre_token_balances {
                log::info!("{:?}", balance);
            }
        } else {
            log::info!("No Pre Token Balances Found.");
        }

        if let OptionSerializer::Some(post_token_balances) = &meta.post_token_balances {
            log::info!("Post Token Balances:");
            for balance in post_token_balances {
                log::info!("{:?}", balance);
            }
        } else {
            log::info!("No Post Token Balances Found.");
        }
    }

    fn detect_balance_changes(
        meta: &UiTransactionStatusMeta,
        account_index: usize,
    ) -> (Option<f64>, Option<f64>) {
        let mut sol_difference = None;
        let mut token_difference = None;

        // Różnice dla SOL
        if let (Some(pre_balance), Some(post_balance)) = (
            meta.pre_balances.get(account_index),
            meta.post_balances.get(account_index),
        ) {
            let difference = *post_balance as i64 - *pre_balance as i64;
            if difference != 0 {
                sol_difference = Some(difference as f64 / 1_000_000_000.0); // Przelicz lamports na SOL
            }
        }

        // Różnice dla SPL Tokenów
        if let (OptionSerializer::Some(pre_token_balances), OptionSerializer::Some(post_token_balances)) =
            (&meta.pre_token_balances, &meta.post_token_balances)
        {
            for pre_balance in pre_token_balances.iter() {
                /*log::info!(
                "Pre Token Balance: Account Index: {}, Mint: {}, Owner: {:?}, Amount: {:?}",
                pre_balance.account_index,
                pre_balance.mint,
                pre_balance.owner,
                pre_balance.ui_token_amount.ui_amount
            );*/

                // Znajdź odpowiadający wpis w `post_token_balances` na podstawie `account_index`, `mint` i `owner`
                if let Some(post_balance) = post_token_balances.iter().find(|post| {
                    post.account_index == pre_balance.account_index
                        && post.mint == pre_balance.mint
                        && post.owner == pre_balance.owner
                }) {
                    /*log::info!("Post Token Balance Found: Account Index: {}, Mint: {}, Owner: {:?}, Amount: {:?}",
                    post_balance.account_index,
                    post_balance.mint,
                    post_balance.owner,
                    post_balance.ui_token_amount.ui_amount
                );*/

                    // Oblicz różnicę
                    if let (Some(pre_amount), Some(post_amount)) = (
                        pre_balance.ui_token_amount.ui_amount,
                        post_balance.ui_token_amount.ui_amount,
                    ) {
                        let difference = post_amount - pre_amount;
                        /*log::info!("Token Difference Calculated: Pre: {}, Post: {}, Difference: {}",
                        pre_amount,
                        post_amount,
                        difference
                    );*/
                        if difference.abs() > 0.0 {
                            token_difference = Some(difference);
                        }
                    }
                } /*else {
                    log::info!(
                    "No matching Post Token Balance Found for Account Index: {}, Mint: {}, Owner: {:?}",
                    pre_balance.account_index,
                    pre_balance.mint,
                    pre_balance.owner
                );
                }*/
            }
        }

        log::info!("SOL Difference: {:?}", sol_difference);
        log::info!("Token Difference: {:?}", token_difference);

        (sol_difference, token_difference)
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
            _ => "1970-01-01 00:00:00".to_string(), // Domyślna data dla błędnych wartości
        }
    }

    fn classify_transaction(
        total_sent_amount: f64,
        total_received_amount: f64,
        sent_currency: Option<String>,
        received_currency: Option<String>,
    ) -> String {
        if total_sent_amount > 0.0 && total_received_amount > 0.0 {
            log::info!(
            "TRADE: Total Sent: {:.9} {}, Total Received: {:.9} {}",
            total_sent_amount,
            sent_currency.clone().unwrap_or_else(|| "Unknown".to_string()),
            total_received_amount,
            received_currency.clone().unwrap_or_else(|| "Unknown".to_string())
        );
            "Trade".to_string()
        } else if total_received_amount > 0.0 {
            log::info!(
            "DEPOSIT: Total Received: {:.9} {}",
            total_received_amount,
            received_currency.clone().unwrap_or_else(|| "Unknown".to_string())
        );
            "Deposit".to_string()
        } else if total_sent_amount > 0.0 {
            log::info!(
            "WITHDRAWAL: Total Sent: {:.9} {}",
            total_sent_amount,
            sent_currency.clone().unwrap_or_else(|| "Unknown".to_string())
        );
            "Withdrawal".to_string()
        } else {
            log::info!("NO TRANSACTION: No relevant data found.");
            "None".to_string()
        }
    }

    fn process_compiled_instructions(
        instructions: &Vec<UiCompiledInstruction>, // Obsługuje Vec
        message: &UiRawMessage,
        meta: &UiTransactionStatusMeta,
        wallet: &Pubkey,
        client: &RpcClient,
        total_sent_amount: &mut f64,
        total_received_amount: &mut f64,
        sent_currency: &mut Option<String>,
        received_currency: &mut Option<String>,
    ) {
        for instruction in instructions {
            if let Some(program_id) = message.account_keys.get(instruction.program_id_index as usize) {
                match program_id.as_str() {
                    "11111111111111111111111111111111" => Self::process_sol_transfer(
                        instruction,
                        message,
                        meta,
                        wallet,
                        total_sent_amount,
                        total_received_amount,
                    ),
                    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" => Self::process_spl_transfer(
                        instruction,
                        message,
                        meta,
                        wallet,
                        client,
                        total_sent_amount,
                        total_received_amount,
                        sent_currency,
                        received_currency,
                    ),
                    _ => {}
                }
            }
        }
    }

    fn process_inner_instructions(
        instructions: &[UiInstruction], // Obsługuje &[UiInstruction]
        message: &UiRawMessage,
        meta: &UiTransactionStatusMeta,
        wallet: &Pubkey,
        client: &RpcClient,
        total_sent_amount: &mut f64,
        total_received_amount: &mut f64,
        sent_currency: &mut Option<String>,
        received_currency: &mut Option<String>,
    ) {
        for instruction in instructions {
            match instruction {
                UiInstruction::Compiled(compiled_instruction) => {
                    if let Some(program_id) = message.account_keys.get(compiled_instruction.program_id_index as usize) {
                        match program_id.as_str() {
                            "11111111111111111111111111111111" => Self::process_sol_transfer(
                                compiled_instruction,
                                message,
                                meta,
                                wallet,
                                total_sent_amount,
                                total_received_amount,
                            ),
                            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" => Self::process_spl_transfer(
                                compiled_instruction,
                                message,
                                meta,
                                wallet,
                                client,
                                total_sent_amount,
                                total_received_amount,
                                sent_currency,
                                received_currency,
                            ),
                            _ => {}
                        }
                    }
                }
                UiInstruction::Parsed(parsed_instruction) => {
                    log::warn!("Parsed instruction not supported yet: {:?}", parsed_instruction);
                }
            }
        }
    }

    fn process_instructions(
        instructions: &[UiInstruction],
        message: &UiRawMessage,
        meta: &UiTransactionStatusMeta,
        wallet: &Pubkey,
        client: &RpcClient,
        total_sent_amount: &mut f64,
        total_received_amount: &mut f64,
        sent_currency: &mut Option<String>,
        received_currency: &mut Option<String>,
    ) {
        for instruction in instructions {
            match instruction {
                UiInstruction::Compiled(compiled_instruction) => {
                    if let Some(program_id) = message.account_keys.get(compiled_instruction.program_id_index as usize) {
                        match program_id.as_str() {
                            "11111111111111111111111111111111" => Self::process_sol_transfer(
                                compiled_instruction,
                                message,
                                meta,
                                wallet,
                                total_sent_amount,
                                total_received_amount,
                            ),
                            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" => Self::process_spl_transfer(
                                compiled_instruction,
                                message,
                                meta,
                                wallet,
                                client,
                                total_sent_amount,
                                total_received_amount,
                                sent_currency,
                                received_currency,
                            ),
                            _ => {}
                        }
                    }
                }
                UiInstruction::Parsed(_) => {
                    log::warn!("Parsed instruction is not supported yet.");
                }
            }
        }
    }

    fn process_sol_transfer(
        instruction: &UiCompiledInstruction,
        message: &UiRawMessage,
        meta: &UiTransactionStatusMeta,
        wallet: &Pubkey,
        total_sent_amount: &mut f64,
        total_received_amount: &mut f64,
    ) {
        if let (Some(source_index), Some(dest_index)) =
            (instruction.accounts.get(0), instruction.accounts.get(1))
        {
            let source = message.account_keys.get(*source_index as usize);
            let dest = message.account_keys.get(*dest_index as usize);

            if source == Some(&wallet.to_string()) {
                if let (Some(pre_balance), Some(post_balance)) = (
                    meta.pre_balances.get(*source_index as usize),
                    meta.post_balances.get(*source_index as usize),
                ) {
                    *total_sent_amount += (*pre_balance as i64 - *post_balance as i64) as f64
                        / 1_000_000_000.0;
                }
            }

            if dest == Some(&wallet.to_string()) {
                if let (Some(pre_balance), Some(post_balance)) = (
                    meta.pre_balances.get(*dest_index as usize),
                    meta.post_balances.get(*dest_index as usize),
                ) {
                    *total_received_amount += (*post_balance as i64 - *pre_balance as i64) as f64
                        / 1_000_000_000.0;
                }
            }
        }
    }

    fn process_spl_transfer(
        instruction: &UiCompiledInstruction,
        message: &UiRawMessage,
        meta: &UiTransactionStatusMeta,
        wallet: &Pubkey,
        client: &RpcClient,
        total_sent_amount: &mut f64,
        total_received_amount: &mut f64,
        sent_currency: &mut Option<String>,
        received_currency: &mut Option<String>,
    ) {
        if let (Some(source_index), Some(dest_index)) =
            (instruction.accounts.get(0), instruction.accounts.get(1))
        {
            let source = message.account_keys.get(*source_index as usize);
            let dest = message.account_keys.get(*dest_index as usize);

            if source == Some(&wallet.to_string()) {
                // Oblicz różnicę tokenów dla source
                Self::calculate_token_difference(
                    meta,
                    *source_index as usize,
                    total_sent_amount,
                    &mut *sent_currency,
                    client,
                );
            }

            if dest == Some(&wallet.to_string()) {
                // Oblicz różnicę tokenów dla dest
                Self::calculate_token_difference(
                    meta,
                    *dest_index as usize,
                    total_received_amount,
                    &mut *received_currency,
                    client,
                );
            }
        }
    }

    fn calculate_token_difference(
        meta: &UiTransactionStatusMeta,
        account_index: usize,
        total_amount: &mut f64,
        currency: &mut Option<String>,
        client: &RpcClient,
    ) {
        if let (OptionSerializer::Some(pre_balances), OptionSerializer::Some(post_balances)) =
            (&meta.pre_token_balances, &meta.post_token_balances)
        {
            for (pre_balance, post_balance) in pre_balances.iter().zip(post_balances.iter()) {
                if pre_balance.account_index as usize == account_index
                    && post_balance.account_index as usize == account_index
                {
                    if let (Some(pre_amount), Some(post_amount)) = (
                        pre_balance.ui_token_amount.ui_amount,
                        post_balance.ui_token_amount.ui_amount,
                    ) {
                        let difference = post_amount - pre_amount;
                        if difference.abs() > 0.0 {
                            *total_amount += difference;

                            // Pobierz nazwę tokena z adresu mint
                            if currency.is_none() {
                                *currency = Self::get_token_symbol_2(client, &post_balance.mint)
                                    .or_else(|| Some("Unknown SPL Token".to_string()));
                            }
                        }
                    }
                }
            }
        }
    }

    fn get_token_symbol_2(client: &RpcClient, mint_address: &str) -> Option<String> {
        // Program ID dla Metaplex Metadata Program
        let metadata_program_id = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

        let program_id = Pubkey::from_str(metadata_program_id).ok()?;
        let mint = Pubkey::from_str(mint_address).ok()?;

        // Oblicz PDA (Program Derived Address) dla metadata konta
        let seeds = &[
            b"metadata".as_ref(),
            program_id.as_ref(),
            mint.as_ref(),
        ];

        let (metadata_pda, _) = Pubkey::find_program_address(seeds, &program_id);

        // Pobierz konto metadata
        if let Ok(account) = client.get_account(&metadata_pda) {
            if let Ok(metadata) =
                mpl_token_metadata::accounts::Metadata::safe_deserialize(&account.data)
            {
                return Some(metadata.symbol.trim().to_string());
            }
        }

        None
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

        log::info!("META: {:?}",meta);
        log::info!("---------------------");
        log::info!("PRB: {:?}",meta.pre_balances);
        log::info!("POB: {:?}",meta.post_balances);
        log::info!("---------------------");

        let raw_message = match &tx.transaction.transaction {
            EncodedTransaction::Json(raw_transaction) => &raw_transaction.message,
            _ => return Err("Unsupported transaction encoding".into()),
        };

        let message = match raw_message {
            UiMessage::Raw(message) => Some(message),
            _ => None,
        }.unwrap();

        log::info!("MESSAGE: {:?}",message);
        //log::info!("---------------------");

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

        log::info!("TX_SRC: {}",tx_src);
        log::info!("TX_DEST: {}",tx_dest);

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
                            log::info!("MPRB: {:?}",meta.pre_balances.get(*source_index as usize));
                            log::info!("MPOB: {:?}",meta.post_balances.get(*source_index as usize));
                            if let (Some(pre_balance), Some(post_balance)) = (
                                meta.pre_balances.get(*source_index as usize),
                                meta.post_balances.get(*source_index as usize),
                            ) {
                                //Theoretically it shouldn't be possible but it is.
                                //There are
                                let amount: i64 = (*pre_balance as i64) - (*post_balance as i64);
                                //if amount != 0 {
                                total_sent_amount += amount as f64 / 1_000_000_000.0;
                                //}
                            }
                            sent_currency = Some("SOL".to_string());
                        }

                        if dest == Some(&wallet.to_string()) {
                            if let (Some(pre_balance), Some(post_balance)) = (
                                meta.pre_balances.get(*dest_index as usize),
                                meta.post_balances.get(*dest_index as usize),
                            ) {
                                let amount = *post_balance as i64 - *pre_balance as i64;
                                total_received_amount += amount as f64 / 1_000_000_000.0;
                            }
                            received_currency = Some("SOL".to_string());
                        }
                    }
                } else if program_id == &token_program_id {
                    // SPL Token transfer

                    let data = instruction.data.as_bytes();
                    let unpacked = TokenInstruction::unpack(data);
                    log::info!("INSTR Unpacked: {:?}",unpacked);

                    match TokenInstruction::unpack(data) {
                        Ok(TokenInstruction::Transfer { amount }) => {
                            println!("INST Amount transferred: {}", amount);
                        }
                        _ => println!("INST Not a transfer instruction."),
                    }

                    if let (Some(source_index), Some(dest_index)) =
                        (instruction.accounts.get(0), instruction.accounts.get(1))
                    {
                        let source = message.account_keys.get(*source_index as usize);
                        let dest = message.account_keys.get(*dest_index as usize);

                        if source == Some(&wallet.to_string()) {
                            match (&meta.pre_token_balances, &meta.post_token_balances) {
                                (OptionSerializer::Some(pre_balances), OptionSerializer::Some(post_balances)) => {
                                    for (pre_balance, post_balance) in pre_balances.iter().zip(post_balances.iter()) {
                                        if let Some(key) = message.account_keys.get(pre_balance.account_index as usize) {
                                            if let Some(src_key) = message.account_keys.get(*source_index as usize) {
                                                if key == src_key {
                                                    // Oblicz różnicę między stanem przed i po transakcji
                                                    if let (Some(pre_amount), Some(post_amount)) = (
                                                        pre_balance.ui_token_amount.ui_amount,
                                                        post_balance.ui_token_amount.ui_amount,
                                                    ) {
                                                        let difference = pre_amount - post_amount;
                                                        if difference > 0.0 {
                                                            log::info!("Token sent: {}", difference);
                                                            total_sent_amount += difference;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }

                            if total_sent_amount > 0.0 {
                                sent_currency = Some(Self::decode_currency(*source_index as usize, &message, &client));
                            }
                        }

/*                        if source == Some(&wallet.to_string()) {
                            match &meta.pre_token_balances {
                                OptionSerializer::Some(pre_balances) => {
                                    for balance in pre_balances {
                                        if let Some(key) = message.account_keys.get(balance.account_index as usize) {
                                            if let Some(src_key) = message.account_keys.get(*source_index as usize) {
                                                if key == src_key {
                                                    if let Some(amount) = balance.ui_token_amount.ui_amount {
                                                        log::info!("Source Balance UI TOKEN: {:?}",balance.ui_token_amount);
                                                        total_sent_amount += amount;
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

                        }*/

                        if dest == Some(&wallet.to_string()) {
                            match (&meta.pre_token_balances, &meta.post_token_balances) {
                                (OptionSerializer::Some(pre_balances), OptionSerializer::Some(post_balances)) => {
                                    for (pre_balance, post_balance) in pre_balances.iter().zip(post_balances.iter()) {
                                        if let Some(key) = message.account_keys.get(pre_balance.account_index as usize) {
                                            if let Some(dst_key) = message.account_keys.get(*dest_index as usize) {
                                                if key == dst_key {
                                                    // Oblicz różnicę między stanem przed i po transakcji
                                                    if let (Some(pre_amount), Some(post_amount)) = (
                                                        pre_balance.ui_token_amount.ui_amount,
                                                        post_balance.ui_token_amount.ui_amount,
                                                    ) {
                                                        let difference = post_amount - pre_amount;
                                                        if difference > 0.0 {
                                                            log::info!("Token received: {}", difference);
                                                            total_received_amount += difference;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }

                            if total_received_amount > 0.0 {
                                received_currency = Some(Self::decode_currency(*dest_index as usize, &message, &client));
                            }
                        }

/*                        if dest == Some(&wallet.to_string()) {
                            match &meta.post_token_balances {
                                OptionSerializer::Some(post_balances) => {
                                    for balance in post_balances {
                                        if let Some(key) = message.account_keys.get(balance.account_index as usize) {
                                            if let Some(dst_key) = message.account_keys.get(*dest_index as usize) {
                                                if key == dst_key {
                                                    if let Some(amount) = balance.ui_token_amount.ui_amount {
                                                        log::info!("Dest Balance UI TOKEN: {:?}",balance.ui_token_amount);
                                                        total_received_amount += amount;
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
                        }*/
                    }
                }
            }
        }

        match &meta.inner_instructions {
            OptionSerializer::Some(inner_instructions) => {
                for inner in inner_instructions {
                    for instruction in &inner.instructions {
                        match instruction {
                            UiInstruction::Compiled(compiled_instruction) => {
                                if let Some(program_id) = message.account_keys.get(compiled_instruction.program_id_index as usize) {
                                    if program_id == &system_program_id {
                                        // SOL transfer
                                        if let (Some(source_index), Some(dest_index)) =
                                            (compiled_instruction.accounts.get(0), compiled_instruction.accounts.get(1))
                                        {
                                            let source = message.account_keys.get(*source_index as usize);
                                            let dest = message.account_keys.get(*dest_index as usize);

                                            if source == Some(&wallet.to_string()) {
                                                if let (Some(pre_balance), Some(post_balance)) = (
                                                    meta.pre_balances.get(*source_index as usize),
                                                    meta.post_balances.get(*source_index as usize),
                                                ) {
                                                    let amount = *pre_balance as i64 - *post_balance as i64;
                                                    total_sent_amount += amount as f64 / 1_000_000_000.0;
                                                }
                                                sent_currency = Some("SOL".to_string());
                                            }

                                            if dest == Some(&wallet.to_string()) {
                                                if let (Some(pre_balance), Some(post_balance)) = (
                                                    meta.pre_balances.get(*dest_index as usize),
                                                    meta.post_balances.get(*dest_index as usize),
                                                ) {
                                                    let amount = *post_balance as i64 - *pre_balance as i64;
                                                    total_received_amount += amount as f64 / 1_000_000_000.0;
                                                }
                                                received_currency = Some("SOL".to_string());
                                            }
                                        }
                                    } else if program_id == &token_program_id {
                                        // SPL Token transfer

                                        let data = compiled_instruction.data.as_bytes();

                                        let unpacked = TokenInstruction::unpack(data);
                                        log::info!("INNER Unpacked: {:?}",unpacked);

                                        match TokenInstruction::unpack(data) {
                                            Ok(TokenInstruction::Transfer { amount }) => {
                                                println!("INNER Amount transferred: {}", amount);
                                            }
                                            _ => println!("INNER Not a transfer instruction."),
                                        }

                                        if let (Some(source_index), Some(dest_index)) =
                                            (compiled_instruction.accounts.get(0), compiled_instruction.accounts.get(1))
                                        {
                                            let source = message.account_keys.get(*source_index as usize);
                                            let dest = message.account_keys.get(*dest_index as usize);

                                            if source == Some(&wallet.to_string()) {
                                                match (&meta.pre_token_balances, &meta.post_token_balances) {
                                                    (OptionSerializer::Some(pre_balances), OptionSerializer::Some(post_balances)) => {
                                                        for (pre_balance, post_balance) in pre_balances.iter().zip(post_balances.iter()) {
                                                            if let Some(key) = message.account_keys.get(pre_balance.account_index as usize) {
                                                                if let Some(src_key) = message.account_keys.get(*source_index as usize) {
                                                                    if key == src_key {
                                                                        // Oblicz różnicę między stanem przed i po transakcji
                                                                        if let (Some(pre_amount), Some(post_amount)) = (
                                                                            pre_balance.ui_token_amount.ui_amount,
                                                                            post_balance.ui_token_amount.ui_amount,
                                                                        ) {
                                                                            let difference = pre_amount - post_amount;
                                                                            if difference > 0.0 {
                                                                                log::info!("Inner Token sent: {}", difference);
                                                                                total_sent_amount += difference;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    _ => {}
                                                }

                                                if total_sent_amount > 0.0 {
                                                    sent_currency = Some(Self::decode_currency(*source_index as usize, &message, &client));
                                                }
                                            }

                                            /*if source == Some(&wallet.to_string()) {
                                                match &meta.pre_token_balances {
                                                    OptionSerializer::Some(pre_balances) => {
                                                        for balance in pre_balances {
                                                            if let Some(key) = message.account_keys.get(balance.account_index as usize) {
                                                                if let Some(src_key) = message.account_keys.get(*source_index as usize) {
                                                                    if key == src_key {
                                                                        if let Some(amount) = balance.ui_token_amount.ui_amount {
                                                                            log::info!("Source Balance UI TOKEN: {:?}",balance.ui_token_amount);
                                                                            total_sent_amount += amount;
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
                                            }*/

                                            if dest == Some(&wallet.to_string()) {
                                                match (&meta.pre_token_balances, &meta.post_token_balances) {
                                                    (OptionSerializer::Some(pre_balances), OptionSerializer::Some(post_balances)) => {
                                                        for (pre_balance, post_balance) in pre_balances.iter().zip(post_balances.iter()) {
                                                            if let Some(key) = message.account_keys.get(pre_balance.account_index as usize) {
                                                                if let Some(dst_key) = message.account_keys.get(*dest_index as usize) {
                                                                    if key == dst_key {
                                                                        // Oblicz różnicę między stanem przed i po transakcji
                                                                        if let (Some(pre_amount), Some(post_amount)) = (
                                                                            pre_balance.ui_token_amount.ui_amount,
                                                                            post_balance.ui_token_amount.ui_amount,
                                                                        ) {
                                                                            let difference = post_amount - pre_amount;
                                                                            if difference > 0.0 {
                                                                                log::info!("Inner Token received: {}", difference);
                                                                                total_received_amount += difference;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    _ => {}
                                                }

                                                if total_received_amount > 0.0 {
                                                    received_currency = Some(Self::decode_currency(*dest_index as usize, &message, &client));
                                                }
                                            }

                                            /*if dest == Some(&wallet.to_string()) {
                                                match &meta.post_token_balances {
                                                    OptionSerializer::Some(post_balances) => {
                                                        for balance in post_balances {
                                                            if let Some(key) = message.account_keys.get(balance.account_index as usize) {
                                                                if let Some(dst_key) = message.account_keys.get(*dest_index as usize) {
                                                                    if key == dst_key {
                                                                        if let Some(amount) = balance.ui_token_amount.ui_amount {
                                                                            log::info!("Dest Balance UI TOKEN: {:?}",balance.ui_token_amount);
                                                                            total_received_amount += amount;
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    },
                                                    OptionSerializer::None => {},
                                                    OptionSerializer::Skip => {}
                                                }
                                                if total_received_amount > 0.0 {
                                                    received_currency = Some(Self::decode_currency(*dest_index as usize, &message, &client));
                                                }
                                            }*/
                                        }
                                    }
                                }
                            },
                            _ => {}  // Handle other variants if needed
                        }
                    }
                }
            },
            OptionSerializer::None => {},
            OptionSerializer::Skip => {}
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

        log::info!("TX: {}",transaction);

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