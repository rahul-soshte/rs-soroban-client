/// Real-world async pattern: A service that needs to build transactions from multiple places
use std::sync::Arc;
use tokio::sync::Mutex;
use soroban_client::{
    account::{Account, AccountBehavior},
    transaction::{TransactionBuilder, TransactionBuilderBehavior, Transaction},
    network::{Networks, NetworkPassphrase},
};

/// A service that manages transactions for a single account
/// This demonstrates how you'd structure code that needs to share an Account
/// across multiple async functions/tasks
pub struct TransactionService {
    account: Arc<Mutex<Account>>,
    network: String,
}

impl TransactionService {
    pub fn new(account_id: &str, sequence: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let account = Account::new(account_id, sequence)?;
        Ok(Self {
            account: Arc::new(Mutex::new(account)),
            network: Networks::testnet().to_string(),
        })
    }

    /// Build a transaction - locks the account for the duration
    pub async fn build_transaction(&self) -> Result<Transaction, Box<dyn std::error::Error>> {
        let mut account = self.account.lock().await;

        let tx = TransactionBuilder::new(&mut *account, &self.network, None)
            .fee(1000u32)
            .set_timeout(30)?
            .build();

        Ok(tx)
    }

    /// Get current sequence number
    pub async fn get_sequence(&self) -> String {
        let account = self.account.lock().await;
        account.sequence_number()
    }

    /// Clone the service for sharing across tasks
    pub fn clone_service(&self) -> Self {
        Self {
            account: self.account.clone(),
            network: self.network.clone(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Real World Pattern: Transaction Service ===\n");

    let service = TransactionService::new(
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        "100",
    )?;

    println!("Initial sequence: {}", service.get_sequence().await);

    // Scenario 1: Sequential transactions from the same service
    println!("\n--- Scenario 1: Sequential transactions ---");
    for i in 1..=3 {
        let tx = service.build_transaction().await?;
        println!("Built transaction {}: sequence = {:?}", i, tx.sequence);
    }

    // Scenario 2: Multiple async tasks sharing the same service
    println!("\n--- Scenario 2: Concurrent tasks (serialized by mutex) ---");
    let mut handles = vec![];

    for i in 1..=5 {
        let service_clone = service.clone_service();
        let handle = tokio::spawn(async move {
            // Each task will acquire the lock when ready, build TX, then release
            let tx = service_clone.build_transaction().await.unwrap();
            println!("Task {} built TX with sequence {:?}", i, tx.sequence);
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await?;
    }

    println!("\nFinal sequence: {}", service.get_sequence().await);

    // Demonstrate the key insight
    println!("\n=== Key Insight ===");
    println!("The mutable reference (&mut Account) ensures:");
    println!("1. Only one transaction can be built at a time (thread-safe by design)");
    println!("2. Sequence numbers are always incremented correctly");
    println!("3. No race conditions possible - Rust's type system prevents them!");
    println!("\nFor async contexts, wrap Account in Arc<Mutex<Account>>.");
    println!("The mutex ensures sequential access even across async tasks.");

    Ok(())
}
