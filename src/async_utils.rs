#[cfg(feature = "with-tokio")]
mod runtime {
    use crate::{
        error::Error,
        soroban_rpc::{GetTransactionResponse, TransactionStatus},
        Server,
    };
    use std::time::{Duration, Instant};
    use tokio::time::sleep;

    pub async fn wait_transaction(
        server: &Server,
        hash: String,
        max_wait: Duration,
    ) -> Result<GetTransactionResponse, (Error, Option<GetTransactionResponse>)> {
        let mut delay = Duration::from_secs(1);
        let start = Instant::now();
        let mut last_response: Option<GetTransactionResponse> = None;

        while start.elapsed() < max_wait {
            match server.get_transaction(&hash).await {
                Ok(tx) => match tx.status {
                    TransactionStatus::Success | TransactionStatus::Failed => {
                        return Ok(tx);
                    }
                    TransactionStatus::NotFound => {
                        last_response = Some(tx);
                        sleep(delay).await;
                        delay = std::cmp::min(delay * 2, Duration::from_secs(60));
                    }
                },
                Err(e) => {
                    return Err((e, last_response));
                }
            }
        }
        Err((
            Error::WaitTransactionTimeout(max_wait.as_secs(), start.elapsed().as_secs()),
            last_response,
        ))
    }
}

#[cfg(not(feature = "with-tokio"))]
mod runtime {
    use crate::{error::Error, soroban_rpc::GetTransactionResponse, Server};
    use std::time::Duration;
    pub async fn wait_transaction(
        _server: &Server,
        _hash: String,
        _max_wait: Duration,
    ) -> Result<GetTransactionResponse, (Error, Option<GetTransactionResponse>)> {
        Err((
            Error::NotImplemented(
                "Function wait_transaction requires the with-tokio feature".into(),
            ),
            None,
        ))
    }
}

pub use runtime::wait_transaction;
