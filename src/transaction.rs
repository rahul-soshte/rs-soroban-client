use core::panic;

use crate::soroban_rpc::soroban_rpc::{
    is_simulation_success, BaseSimulateTransactionResponse, RawSimulateTransactionResponse,
    RestorePreamble, SimulateHostFunctionResult, SimulateTransactionErrorResponse,
    SimulateTransactionResponse, SimulateTransactionRestoreResponse,
    SimulateTransactionSuccessResponse,
};
use stellar_baselib::transaction_builder::TransactionBuilderBehavior;
pub use stellar_baselib::{
    account::Account,
    soroban_data_builder::{self, SorobanDataBuilder},
    transaction::Transaction,
    transaction_builder::TransactionBuilder,
};
use stellar_baselib::account::AccountBehavior;

use stellar_xdr::next::{DiagnosticEvent, ReadXdr, ScVal, SorobanAuthorizationEntry, Limits};
use stellar_baselib::soroban_data_builder::SorobanDataBuilderBehavior;

pub enum SimulationResponse {
    Normal(SimulateTransactionResponse),
    Raw(RawSimulateTransactionResponse),
}
pub fn assemble_transaction(
    raw: Transaction,
    network_passphrase: &str,
    simulation: SimulationResponse,
) -> Result<TransactionBuilder, String> {
    if !is_soroban_transaction(&raw) {
        return Err("unsupported transaction: must contain exactly one invokeHostFunction, bumpFootprintExpiration, or restoreFootprint operation".to_string());
    }

    let val = match simulation {
        SimulationResponse::Normal(x) => Either::Left(x),
        SimulationResponse::Raw(x) => Either::Right(x),
    };

    let success = parse_raw_simulation(val);

    if !is_simulation_success(&success) {
        return Err(format!("simulation incorrect: {:?}", success));
    }

    let classic_fee_num = raw.fee;
    let min_resource_fee_num = match success {
        SimulateTransactionResponse::Success(ref x) => {
            x.min_resource_fee.parse::<u32>().unwrap_or(0)
        }
        _ => panic!("Invalid"),
    };

    let soroban_tx_data = match success {
        SimulateTransactionResponse::Success(x) => x.clone(),
        SimulateTransactionResponse::Restore(_) => todo!(),
        SimulateTransactionResponse::Error(_) => todo!(),
    };

    let _ss = soroban_tx_data.transaction_data.build();

    let source = raw.source;
    let txn_builder =
        TransactionBuilder::new(Account::new(&source, "0").unwrap(), network_passphrase)
            .fee(classic_fee_num + min_resource_fee_num)
            .clone();
    // .build();
    let val = raw.operations.unwrap()[0].clone();
    match val.clone() {
        #[allow(non_snake_case)]
        InvokeHostFunctionOp => {
            // txn_builder.clear_operations();
            let _invoke_op = val;
            // let existing_auth = match &invoke_op. {
            //     // Some(auth) => auth,
            //     // None => &Vec::new(),
            // };
            txn_builder.clone().add_operation(InvokeHostFunctionOp);
            // txn_builder.add_operation(Operation::invoke_host_function(
            //     invoke_op.source.clone(),
            //     invoke_op.func.clone(),
            //     if !existing_auth.is_empty() {
            //         existing_auth.clone()
            //     } else {
            //         success.result.unwrap().auth.clone()
            //     }
            // ));
        }

        _ => panic!("Invalid"),
    }
    Ok(txn_builder.clone())
}

pub fn parse_raw_simulation(
    sim: Either<SimulateTransactionResponse, RawSimulateTransactionResponse>,
) -> SimulateTransactionResponse {
    if !is_simulation_raw(sim.clone()) {
        match sim {
            Either::Left(x) => return x.clone(),
            Either::Right(_) => panic!("Invalid"),
        }
    }

    let base = BaseSimulateTransactionResponse {
        _parsed: true,
        id: match &sim {
            Either::Right(raw) => raw.id.clone(),
            _ => panic!("Unexpected type"), // or return some error
        },
        latest_ledger: match &sim {
            Either::Right(raw) => raw.latest_ledger.clone(),
            _ => panic!("Unexpected type"), // or return some error
        },
        events: match &sim {
            Either::Right(raw) => raw
                .events
                .as_ref()
                .unwrap_or(&vec![])
                .iter()
                .map(|evt| DiagnosticEvent::from_xdr_base64(evt, Limits::none()).unwrap())
                .collect(),
            _ => vec![],
        },
    };

    match &sim {
        Either::Right(raw) => {
            if let Some(err) = &raw.error {
                SimulateTransactionResponse::Error(SimulateTransactionErrorResponse {
                    base,
                    error: err.to_string(),
                })
            } else {
                parse_successful(raw.clone(), base)
            }
        }
        _ => panic!("Unexpected type"), // or return some error
    }
}

fn parse_successful(
    sim: RawSimulateTransactionResponse,
    partial: BaseSimulateTransactionResponse,
) -> SimulateTransactionResponse {
    let success_data = {
        let mut base = SimulateTransactionSuccessResponse {
            transaction_data: SorobanDataBuilder::new(Some(soroban_data_builder::Either::Left(
                sim.transaction_data.unwrap(),
            ))),
            min_resource_fee: sim.min_resource_fee.unwrap(),
            cost: sim.cost.unwrap(),
            result: None,
            base: partial,
            latest_ledger: 3,
        };

        if let Some(results) = &sim.results {
            if !results.is_empty() {
                base.result = Some(SimulateHostFunctionResult {
                    auth: results[0]
                        .auth
                        .as_ref()
                        .unwrap_or(&vec![])
                        .iter()
                        .map(|entry| SorobanAuthorizationEntry::from_xdr_base64(entry, Limits::none()).unwrap())
                        .collect(),
                    retval: if let Some(xdr) = &results[0].xdr {
                        ScVal::from_xdr_base64(xdr, Limits::none()).unwrap() // assuming ScVal is defined elsewhere
                    } else {
                        ScVal::Void
                    },
                });
            }
        }

        base
    };

    match &sim.restore_preamble {
        Some(preamble) => {
            SimulateTransactionResponse::Restore(SimulateTransactionRestoreResponse {
                restore_preamble: RestorePreamble {
                    min_resource_fee: preamble.min_resource_fee.clone(),
                    transaction_data: preamble.transaction_data.clone(),
                },
                base: success_data.clone(),
                result: success_data.result,
            })
        }
        _ => SimulateTransactionResponse::Success(success_data),
    }
}

pub fn is_simulation_raw(
    sim: Either<SimulateTransactionResponse, RawSimulateTransactionResponse>,
) -> bool {
    match sim {
        Either::Left(response) => match response {
            SimulateTransactionResponse::Success(x) => x.base._parsed,
            SimulateTransactionResponse::Restore(x) => x.base.base._parsed,
            SimulateTransactionResponse::Error(x) => x.base._parsed,
        },
        Either::Right(_) => true,
    }
}

fn is_soroban_transaction(tx: &Transaction) -> bool {
    if tx.operations.clone().unwrap().len() != 1 {
        return false;
    }

    match tx.operations.clone().unwrap()[0].clone() {
        #[allow(non_snake_case)]
        _InvokeHostFunctionOp => true,
        #[allow(non_snake_case)]
        _BumpFootprintExpirationOp => true,
        #[allow(non_snake_case)]
        _RestoreFootprintOp => true,
        _ => false,
    }
}

#[derive(Clone)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}
