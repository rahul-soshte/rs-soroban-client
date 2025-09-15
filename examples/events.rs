use soroban_client::{soroban_rpc::EventType, EventFilter, Options};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_url = "https://soroban-testnet.stellar.org";
    let server = soroban_client::Server::new(server_url, Options::default())?;
    let contract_id = "CCB3TAFLJBQ7BVRIYNZZAUP3SDG7LR3AHCZANTD7GU2FJAU6MZ63XCN3";
    let event_filter = EventFilter::new(EventType::All).contract(contract_id);
    let ledger = 541800;
    let events_response = server
        .get_events(
            soroban_client::Pagination::From(ledger),
            vec![event_filter],
            Some(100),
        )
        .await?;

    let count = events_response.events.len();
    println!("Got {count} events since {ledger}");
    for e in events_response.events.iter() {
        println!("topic: {:?}, value: {:?}", e.topic(), e.value());
    }

    Ok(())
}
