use core::panic;
use std::{cell::RefCell, rc::Rc};

use crate::soroban_rpc::soroban_rpc::{
    is_simulation_success, BaseSimulateTransactionResponse, RawSimulateTransactionResponse,
    RestorePreamble, SimulateHostFunctionResult, SimulateTransactionErrorResponse,
    SimulateTransactionResponse, SimulateTransactionRestoreResponse,
    SimulateTransactionSuccessResponse,
};
use stellar_baselib::account::AccountBehavior;
use stellar_baselib::transaction_builder::TransactionBuilderBehavior;
pub use stellar_baselib::{
    account::Account,
    soroban_data_builder::{self, SorobanDataBuilder},
    transaction::Transaction,
    transaction::TransactionBehavior,
    transaction_builder::TransactionBuilder,
};

use stellar_baselib::soroban_data_builder::SorobanDataBuilderBehavior;
use stellar_baselib::xdr::xdr::next::{
    DiagnosticEvent, Limits, ReadXdr, ScVal, SorobanAuthorizationEntry,
};

// use stellar_baselib::operation::Operation

pub enum SimulationResponse {
    Normal(SimulateTransactionResponse),
    Raw(RawSimulateTransactionResponse),
}

//TODO: Assemble Transaction Tests
pub fn assemble_transaction(
    raw: Transaction,
    network_passphrase: &str,
    simulation: SimulationResponse,
) -> Result<TransactionBuilder, String> {
    // Ensure the transaction is a valid Soroban transaction
    if !is_soroban_transaction(&raw) {
        return Err("unsupported transaction: must contain exactly one invokeHostFunction, extendFootprintTtl, or restoreFootprint operation".to_string());
    }

    // Parse simulation response and ensure it's successful
    let success = parse_raw_simulation(match simulation {
        SimulationResponse::Normal(sim) => Either::Left(sim),
        SimulationResponse::Raw(raw_sim) => Either::Right(raw_sim),
    });

    if !is_simulation_success(&success) {
        return Err(format!("simulation incorrect: {:?}", success));
    }

    // Calculate fees
    let classic_fee_num = raw.fee;

    let (min_resource_fee, soroban_tx_data, auth): (
        _,
        _,
        Option<stellar_baselib::xdr::xdr::next::VecM<SorobanAuthorizationEntry>>,
    ) = match &success {
        SimulateTransactionResponse::Success(response) => {
            //
            (
                response.min_resource_fee.parse::<u32>().unwrap_or(0),
                response.transaction_data.build(),
                response.result.as_ref().map(|result| {
                    result
                        .auth
                        .clone()
                        .try_into()
                        .expect("Conversion to VecM failed")
                }),
            )
        }
        _ => return Err("Simulation result is not a success".to_string()),
    };

    // Create a transaction builder with updated fees and Soroban data
    let source_acc = Rc::new(RefCell::new(
        Account::new(
            &raw.source.ok_or("missing source account")?,
            &raw.sequence.unwrap(),
        )
        .expect("Failed to copy source account data"),
    ));

    let mut tx_builder =
        TransactionBuilder::new(source_acc.clone(), network_passphrase, raw.time_bounds);

    tx_builder
        .fee(classic_fee_num + min_resource_fee)
        .set_soroban_data(soroban_tx_data);

    // Process the operation
    if let Some(ops) = raw.operations {
        if let stellar_baselib::xdr::xdr::next::OperationBody::InvokeHostFunction(
            invoke_host_function_op,
        ) = ops[0].clone().body
        {
            tx_builder.add_operation(
                stellar_baselib::operation::Operation::invoke_host_function(
                    invoke_host_function_op.host_function,
                    auth,
                    None,
                )
                .unwrap(),
            );
        }
    }

    Ok(tx_builder)
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
        latest_ledger: match &sim {
            Either::Right(raw) => raw.latestLedger,
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
                sim.transactionData.unwrap(),
            ))),
            min_resource_fee: sim.minResourceFee.unwrap(),
            // cost: sim.cost.unwrap(),
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
                        .map(|entry| {
                            SorobanAuthorizationEntry::from_xdr_base64(entry, Limits::none())
                                .unwrap()
                        })
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

    match &sim.restorePreamble {
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

#[derive(Clone, Debug)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}
