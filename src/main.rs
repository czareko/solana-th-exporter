mod domain;
mod ports;

use std::str::FromStr;
use clap::Parser;
use solana_sdk::pubkey::Pubkey;
use crate::ports::{FileExporterService, SolanaTHService};

#[tokio::main]
async fn main(){
    env_logger::init();
    // Parse command-line arguments
    let args = Cli::parse();

    // Check if the address was provided
    let address = match args.address {
        Some(addr) => addr,
        None => {
            log::error!("Error: Missing required parameter `-a` or `--address`.");
            log::error!("Usage: ./solana-exporter -a <Solana Wallet Address>");
            std::process::exit(1);
        }
    };

    // Validate the provided Solana address
    match validate_address(&address) {
        Ok(valid_address) => {
            log::info!("Fetching transaction history for address: {}", valid_address);
            let transactions = match args.operation_limit {
                Some(limit) => {
                    log::info!("Operation limit provided: {}", limit);
                    SolanaTHService::fetch_transactions(valid_address, limit)
                }
                None => {
                    log::info!("No operation limit provided, fetching all transactions.");
                    SolanaTHService::fetch_transactions(valid_address,0)
                }
            };

            // Proceed with fetching and exporting transactions...
            if transactions.len() > 0 {
                let _ = FileExporterService::save_transactions_to_csv(transactions,"transactions.csv");
            }
            else{
                log::info!("No transactions to export");
            }

        }
        Err(err) => {
            log::error!("Error: {}", err);
            std::process::exit(1);
        }
    }
}

fn validate_address(address: &str) -> Result<Pubkey, String> {
    Pubkey::from_str(address).map_err(|_| format!("Invalid Solana address: {}", address))
}

#[derive(Parser)]
#[command(name = "Solana TH Exporter", version = "0.1", author = "Cezary Olborski")]
#[command(about = "Exports Solana transaction history to CSV", long_about = None)]
struct Cli {
    #[arg(short, long, help = "The Solana wallet address to export transactions for")]
    address: Option<String>,

    #[arg(
        short,
        long,
        help = "Limit the number of transactions to fetch (must be > 0)",
        value_parser = parse_positive_integer
    )]
    operation_limit: Option<usize>,
}

fn parse_positive_integer(v: &str) -> Result<usize, String> {
    match v.parse::<usize>() {
        Ok(value) if value > 0 => Ok(value),
        _ => Err("The operation limit must be a positive integer.".to_string()),
    }
}