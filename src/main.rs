mod domain;
mod ports;

use std::str::FromStr;
use clap::Parser;
use log::log;
use solana_sdk::pubkey::Pubkey;
use crate::ports::SolanaTHService;

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
            let _ = SolanaTHService::fetch_transactions(valid_address);

            // Proceed with fetching and exporting transactions...
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
#[command(name = "Solana Exporter", version = "1.0", author = "Your Name")]
#[command(about = "Exports Solana transaction history to CSV", long_about = None)]
struct Cli {
    #[arg(short, long, help = "The Solana wallet address to export transactions for")]
    address: Option<String>,
}