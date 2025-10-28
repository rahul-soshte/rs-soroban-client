/// This example demonstrates different patterns for using Account in async contexts
use std::sync::Arc;
use tokio::sync::Mutex;
use soroban_client::{
    account::{Account, AccountBehavior},
    transaction::{TransactionBuilder, TransactionBuilderBehavior},
    network::{Networks, NetworkPassphrase},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Pattern 1: Simple Sequential (Already Works) ===");
    pattern_1_sequential().await?;

    println!("\n=== Pattern 2: Shared Account with Arc<Mutex<Account>> ===");
    pattern_2_shared_mutex().await?;

    println!("\n=== Pattern 3: Multiple Tasks with Same Account ===");
    pattern_3_multiple_tasks().await?;

    Ok(())
}

/// Pattern 1: Simple sequential usage (this already works great!)
async fn pattern_1_sequential() -> Result<(), Box<dyn std::error::Error>> {
    let mut account = Account::new(
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        "100",
    )?;

    // Build transaction 1
    let tx1 = TransactionBuilder::new(&mut account, Networks::testnet(), None)
        .fee(1000u32)
        .set_timeout(30)?
        .build();
    println!("TX1 sequence: {:?}", tx1.sequence);

    // Simulate async work
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Build transaction 2
    let tx2 = TransactionBuilder::new(&mut account, Networks::testnet(), None)
        .fee(1000u32)
        .set_timeout(30)?
        .build();
    println!("TX2 sequence: {:?}", tx2.sequence);

    Ok(())
}

/// Pattern 2: Sharing account across async boundaries with Arc<Mutex<Account>>
/// This is what you'd need if the account needs to be shared
async fn pattern_2_shared_mutex() -> Result<(), Box<dyn std::error::Error>> {
    let account = Account::new(
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        "200",
    )?;

    // Wrap in Arc<Mutex<>> for sharing across async tasks
    let shared_account = Arc::new(Mutex::new(account));

    // Clone the Arc for use in async context
    let account_clone = shared_account.clone();

    // Build transaction in async block
    let tx = tokio::spawn(async move {
        let mut account_guard = account_clone.lock().await;
        TransactionBuilder::new(&mut *account_guard, Networks::testnet(), None)
            .fee(1000u32)
            .set_timeout(30)
            .unwrap()
            .build()
    }).await?;

    println!("TX sequence from spawned task: {:?}", tx.sequence);

    Ok(())
}

/// Pattern 3: Multiple tasks need to build transactions sequentially
async fn pattern_3_multiple_tasks() -> Result<(), Box<dyn std::error::Error>> {
    let account = Account::new(
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        "300",
    )?;

    let shared_account = Arc::new(Mutex::new(account));

    // Spawn multiple tasks that each build a transaction
    let mut handles = vec![];

    for i in 0..3 {
        let account_clone = shared_account.clone();
        let handle = tokio::spawn(async move {
            // Lock the account
            let mut account_guard = account_clone.lock().await;

            // Build transaction
            let tx = TransactionBuilder::new(&mut *account_guard, Networks::testnet(), None)
                .fee(1000u32)
                .set_timeout(30)
                .unwrap()
                .build();

            println!("Task {}: Built TX with sequence {:?}", i, tx.sequence);

            // Important: lock is released here when account_guard is dropped
            tx
        });

        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await?;
    }

    Ok(())
}
