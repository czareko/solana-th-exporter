# Solana Transaction History Exporter

Solana Transaction History Exporter is a command-line tool designed to fetch and export transaction data from the Solana blockchain. It retrieves transaction history for a given wallet address and outputs the results in CSV format. The tool is lightweight, Docker-compatible, and easily extensible.


!!! This is just a proof of concept. Retrieving such a transaction history in detail would require more extensive analysis. !!!
<hr/>

### Features

- Fetch transaction history for any Solana wallet address. 
- Filter transactions based on the number of operations (operation_limit). 
- Export data to CSV format for further analysis. 
- Dockerized for deployment in isolated environments.


<hr/>

### Business TOOD

1.	**Deeper analysis of operation types:**
   - Extend the classification of transaction types (e.g., trades, deposits, withdrawals). 
   - Identify and handle special cases like multi-step transactions.
2.	**Improved token metadata analysis:**
- Fetch richer metadata for tokens (e.g., token names, symbols, decimals). 
- Use decentralized APIs to enhance accuracy.
3.	**Parallel data fetching:**
- Enable fetching transactions from multiple RPC endpoints simultaneously to improve performance.

### Technical TODO

1.	**Token metadata caching:**
- Implement a local cache to store token metadata.
2.	**Unit tests:**
- Add unit tests to ensure the correctness of transaction parsing and CSV generation.
3.	**Smaller, better-defined functions:**
- Refactor large functions to improve readability and maintainability.

<hr/>

### Prerequisites

1. Rust and Cargo:
- Install Rust and Cargo using the official installation script:
```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

- 	Restart your shell or run:
```shell
source $HOME/.cargo/env
```

- Verify installation:
```shell
rustc --version
cargo --version
```


2. Docker:
 - Install Docker by following the instructions on the official Docker website. 
 - Ensure Docker Compose is installed and available in your system.
3.	Solana Blockchain RPC Endpoint:
 - The application defaults to the public Solana RPC endpoint.

<hr/>

### Local Installation

1. Clone the repository:

```shell
git clone <repository_url>
cd solana-th-exporter
```

2. Build the project:

```shell
cargo build --release
```

3. Run the application:

```shell
RUST_LOG=info ./target/release/solana-th-exporter -a <wallet_address> -o <operation_limit>
```

## Command-Line Usage

```shell
RUST_LOG=info solana-th-exporter -a <wallet_address> [-o <operation_limit>]
```

Parameters:

-a, --address (Required): The Solana wallet address to fetch transactions for.

-o, --operation-limit (Optional): The maximum number of transactions to process. Defaults to unlimited.

#### Examples:


1.	Fetch all transactions for a wallet:

```shell
solana-th-exporter -a D3utWrchKzcSZM9HxoXkpGfYhMVzFabkf5NQvSKDUYJ5
```

2.	Fetch the first 10 transactions:

```shell
solana-th-exporter -a D3utWrchKzcSZM9HxoXkpGfYhMVzFabkf5NQvSKDUYJ5 -o 10
```
<hr/>

### Output Format

The application generates a CSV file named transactions.csv with the following columns:

- Date: Timestamp of the transaction.
- Tx Hash: Unique transaction identifier.
- Source: Source address of the transaction.
- Destination: Destination address of the transaction.
- Sent Amount: Amount sent in the transaction.
- Sent Currency: Currency of the sent amount (e.g., SOL, USDC).
- Received Amount: Amount received in the transaction.
- Received Currency: Currency of the received amount.
- Fee Amount: Transaction fee.
- Fee Currency: Currency of the fee (always SOL).

<hr/>

#### Docker Installation

1.	Build the Docker image:

```shell
docker build -t solana-th-exporter .
```

2.  Run the container:

```shell
docker run --rm solana-th-exporter -a <wallet_address> -o <operation_limit>
```

#### Using Docker Compose

1.	Start the service using docker-compose:

```shell
docker-compose up --build

```


2.	The output CSV file will be saved in the project directory as transactions.csv.

<hr/>


### License

This project is licensed under the Apache 2.0 License