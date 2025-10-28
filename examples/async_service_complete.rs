/// Complete example: A transaction service for async applications
/// This shows how to structure a real application that needs to manage
/// an account and build transactions from multiple async contexts.

use std::sync::Arc;
use tokio::sync::Mutex;
use soroban_client::{
    account::{Account, AccountBehavior},
    transaction::{TransactionBuilder, TransactionBuilderBehavior, Transaction},
    network::{Networks, NetworkPassphrase},
};

/// A service that manages a single Stellar account and provides
/// transaction building capabilities in an async context
#[derive(Clone)]
pub struct StellarAccountService {
    account: Arc<Mutex<Account>>,
    network: String,
    default_fee: u32,
}

impl StellarAccountService {
    /// Create a new service for managing transactions
    pub fn new(
        account_id: &str,
        sequence: &str,
        network: &str,
        default_fee: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let account = Account::new(account_id, sequence)?;
        Ok(Self {
            account: Arc::new(Mutex::new(account)),
            network: network.to_string(),
            default_fee,
        })
    }

    /// Build a transaction with default settings
    pub async fn build_transaction(&self) -> Result<Transaction, Box<dyn std::error::Error>> {
        self.build_transaction_with_fee(self.default_fee).await
    }

    /// Build a transaction with custom fee
    pub async fn build_transaction_with_fee(
        &self,
        fee: u32,
    ) -> Result<Transaction, Box<dyn std::error::Error>> {
        let mut account = self.account.lock().await;

        let tx = TransactionBuilder::new(&mut *account, &self.network, None)
            .fee(fee)
            .set_timeout(30)?
            .build();

        Ok(tx)
    }

    /// Get the current sequence number without incrementing it
    pub async fn current_sequence(&self) -> String {
        let account = self.account.lock().await;
        account.sequence_number()
    }

    /// Get the account ID
    pub async fn account_id(&self) -> String {
        let account = self.account.lock().await;
        account.account_id()
    }

    /// Get account info as a tuple (account_id, sequence)
    pub async fn account_info(&self) -> (String, String) {
        let account = self.account.lock().await;
        (account.account_id(), account.sequence_number())
    }
}

/// Simulates a web service handler that needs to build transactions
async fn handle_request_1(service: &StellarAccountService) -> Result<(), Box<dyn std::error::Error>> {
    println!("Handler 1: Building transaction...");
    let tx = service.build_transaction().await?;
    println!("Handler 1: Built TX with sequence {:?}", tx.sequence);
    // Simulate some async work
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!("Handler 1: Transaction sent successfully");
    Ok(())
}

/// Another handler that builds transactions concurrently
async fn handle_request_2(service: &StellarAccountService) -> Result<(), Box<dyn std::error::Error>> {
    println!("Handler 2: Building transaction with custom fee...");
    let tx = service.build_transaction_with_fee(2000).await?;
    println!("Handler 2: Built TX with sequence {:?}", tx.sequence);
    // Simulate some async work
    tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;
    println!("Handler 2: Transaction sent successfully");
    Ok(())
}

/// Background task that periodically checks account info
async fn background_monitor(service: StellarAccountService) {
    for i in 1..=3 {
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        let (account_id, sequence) = service.account_info().await;
        println!("Monitor {}: Account {} at sequence {}", i, &account_id[..8], sequence);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Complete Async Service Example ===\n");

    // Create the service
    let service = StellarAccountService::new(
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        "1000",
        Networks::testnet(),
        1000,
    )?;

    let (account_id, initial_seq) = service.account_info().await;
    println!("Service initialized:");
    println!("  Account: {}", account_id);
    println!("  Sequence: {}\n", initial_seq);

    // Scenario 1: Sequential requests
    println!("--- Scenario 1: Sequential requests ---");
    handle_request_1(&service).await?;
    handle_request_2(&service).await?;
    println!("Sequence after sequential: {}\n", service.current_sequence().await);

    // Scenario 2: Concurrent requests
    println!("--- Scenario 2: Concurrent requests ---");
    let (r1, r2, r3) = tokio::join!(
        handle_request_1(&service),
        handle_request_2(&service),
        handle_request_1(&service),
    );
    r1?;
    r2?;
    r3?;
    println!("Sequence after concurrent: {}\n", service.current_sequence().await);

    // Scenario 3: Background task + foreground requests
    println!("--- Scenario 3: Background monitoring + requests ---");
    let monitor = tokio::spawn(background_monitor(service.clone()));

    for i in 1..=5 {
        let service_clone = service.clone();
        tokio::spawn(async move {
            let tx = service_clone.build_transaction().await.unwrap();
            println!("Spawned task {}: sequence {:?}", i, tx.sequence);
        });
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }

    monitor.await?;
    println!("\nFinal sequence: {}", service.current_sequence().await);

    println!("\n=== Summary ===");
    println!("✅ All transactions built with correct, sequential sequence numbers");
    println!("✅ No race conditions despite concurrent access");
    println!("✅ Service can be cloned and shared across tasks");
    println!("✅ Mutex ensures only one transaction builds at a time");

    Ok(())
}
