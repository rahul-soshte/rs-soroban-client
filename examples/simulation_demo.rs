/// This example demonstrates the difference between `build()` and `build_for_simulation()`
/// when creating transactions for the Stellar/Soroban network.
///
/// Key takeaways:
/// - Use `build()` when you're going to submit the transaction to the network
/// - Use `build_for_simulation()` when you only want to simulate or preview the transaction
/// - `build()` increments the account's sequence number, `build_for_simulation()` does not
///
/// Run with: cargo run --example simulation_demo
use soroban_client::{
    account::{Account, AccountBehavior},
    asset::AssetBehavior,
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::Operation,
    transaction::{TransactionBuilder, TransactionBuilderBehavior},
    Options, Server,
};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_url = "https://soroban-testnet.stellar.org";
    let server = Server::new(server_url, Options::default())?;

    let source_keypair = Keypair::random()?;
    let source_public_key = &source_keypair.public_key();

    // Fund the account using friendbot
    println!("Funding account via friendbot...");
    let account_data = server.request_airdrop(source_public_key).await?;

    let mut source_account = Account::new(source_public_key, &account_data.sequence_number())?;

    let initial_sequence = source_account.sequence_number();
    println!("\nInitial account sequence number: {}", initial_sequence);

    // Create a simple payment operation
    let destination = "GDJJRRMBK4IWLEPJGIE6SXD2LP7REGZODU7WDC3I2D6MR37F4XSHBKX2";
    let payment_op = Operation::new()
        .payment(
            destination,
            &soroban_client::asset::Asset::native(),
            10_000_000, // 1 XLM (7 zeros)
        )
        .expect("Cannot create payment operation");

    // Example 1: Using build_for_simulation() - does NOT increment sequence
    println!("\n=== Example 1: Using build_for_simulation() ===");
    {
        let mut builder = TransactionBuilder::new(&mut source_account, Networks::testnet(), None);
        builder
            .fee(1000u32)
            .add_operation(payment_op.clone())
            .set_timeout(30)
            .unwrap();

        // Build for simulation - this does NOT increment the account sequence number
        let tx_for_simulation = builder.build_for_simulation();

        println!(
            "Transaction sequence number: {}",
            tx_for_simulation.sequence.as_ref().unwrap()
        );
        println!(
            "Account sequence after build_for_simulation(): {}",
            source_account.sequence_number()
        );

        // You can now simulate this transaction without affecting the account state
        match server.simulate_transaction(&tx_for_simulation, None).await {
            Ok(sim_result) => {
                println!("Simulation successful!");
                println!("  - Cost: {:?}", sim_result.min_resource_fee);
                println!("  - Latest ledger: {}", sim_result.latest_ledger);
            }
            Err(e) => {
                println!("Simulation failed (expected on random account): {:?}", e);
            }
        }
    }

    println!(
        "\nAccount sequence after simulation: {}",
        source_account.sequence_number()
    );
    println!("Notice: Sequence number is still {}", initial_sequence);

    // Example 2: Using build() - DOES increment sequence
    println!("\n=== Example 2: Using build() for actual submission ===");
    {
        let mut builder = TransactionBuilder::new(&mut source_account, Networks::testnet(), None);
        builder
            .fee(1000u32)
            .add_operation(payment_op.clone())
            .set_timeout(30)
            .unwrap();

        // Build for actual submission - this INCREMENTS the account sequence number
        let tx_for_submission = builder.build();

        println!(
            "Transaction sequence number: {}",
            tx_for_submission.sequence.as_ref().unwrap()
        );
        println!(
            "Account sequence after build(): {}",
            source_account.sequence_number()
        );
        println!(
            "Notice: Sequence number was incremented from {} to {}",
            initial_sequence,
            source_account.sequence_number()
        );

        // Now if you submit this transaction, the sequence number is correct
        // (we won't actually submit it in this example)
    }

    // Example 3: Multiple simulations without affecting sequence
    println!("\n=== Example 3: Multiple simulations ===");
    println!("Starting sequence: {}", source_account.sequence_number());

    for i in 1..=3 {
        let mut builder = TransactionBuilder::new(&mut source_account, Networks::testnet(), None);
        builder
            .fee(1000u32 * i)
            .add_operation(payment_op.clone())
            .set_timeout(30)
            .unwrap();

        let tx = builder.build_for_simulation();
        println!(
            "Simulation #{}: tx seq = {}, account seq = {}",
            i,
            tx.sequence.as_ref().unwrap(),
            source_account.sequence_number()
        );
    }

    println!("\n✅ Summary:");
    println!("   - build_for_simulation(): Safe for read-only operations, doesn't mutate account");
    println!("   - build(): Required for actual submission, increments sequence number");
    println!("   - Use build_for_simulation() for previewing, testing, or fee estimation");
    println!("   - Use build() when you're ready to submit to the network");

    Ok(())
}
