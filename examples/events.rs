use soroban_client::{soroban_rpc::EventType, EventFilter, Options};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_url = "https://soroban-testnet.stellar.org";
    let server = soroban_client::Server::new(server_url, Options::default())?;

    // XLM contract
    let contract_id = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    let latest_ledger = server.get_latest_ledger().await?.sequence;
    let ledger = latest_ledger - (3600 / 5); // One day of events

    let events_response = server
        .get_events(
            soroban_client::Pagination::From(ledger),
            vec![EventFilter::new(EventType::All).contract(contract_id)],
            Some(100),
        )
        .await?;

    let mut max_ledger = ledger;
    let count = events_response.events.len();
    println!("Got {count} events");
    for e in events_response.events.iter() {
        println!(
            "ledger: {}, topic: {:?}, value: {:?}",
            e.ledger,
            e.topic(),
            e.value()
        );
        max_ledger = e.ledger as u32;
    }

    let mut cursor = events_response.cursor;
    while let Some(token) = cursor {
        let events_response = server
            .get_events(
                soroban_client::Pagination::Cursor(token),
                vec![EventFilter::new(EventType::All).contract(contract_id)],
                None,
            )
            .await?;

        let count = events_response.events.len();
        println!("Got {count}");
        for e in events_response.events.iter() {
            println!(
                "ledger: {}, topic: {:?}, value: {:?}",
                e.ledger,
                e.topic(),
                e.value()
            );
            max_ledger = e.ledger as u32;
        }
        cursor = events_response.cursor;

        if max_ledger >= latest_ledger {
            break;
        }
    }

    Ok(())
}
