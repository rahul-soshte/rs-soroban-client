use std::{cell::RefCell, rc::Rc};

use crate::{error::Error, soroban_rpc::*};
pub use stellar_baselib::{
    account::Account,
    account::AccountBehavior,
    soroban_data_builder::{self, SorobanDataBuilder},
    transaction::Transaction,
    transaction::TransactionBehavior,
    transaction_builder::TransactionBuilder,
    transaction_builder::TransactionBuilderBehavior,
    xdr::{
        DiagnosticEvent, Limits, OperationBody, OperationType, ReadXdr, ScVal,
        SorobanAuthorizationEntry, VecM,
    },
};

// use stellar_baselib::operation::Operation

//TODO: Assemble Transaction Tests
pub fn assemble_transaction(
    tx: Transaction,
    network_passphrase: &str,
    simulation: SimulateTransactionResponse,
) -> Result<TransactionBuilder, Error> {
    // Ensure the transaction is a valid Soroban transaction
    if !is_soroban_transaction(&tx) {
        return Err(Error::InvalidSorobanTransaction);
    }

    if let Some(_error) = simulation.error {
        return Err(Error::SimulationFailed);
    }

    if let Some((_, _restore)) = simulation.to_restore_transaction_data() {
        return Err(Error::RestorationRequired);
    }

    // Calculate fees
    let classic_fee_num = tx.fee;

    let auth = if let Some((_, a)) = simulation.to_result() {
        Some(a.try_into().expect("Cannot convert Vec to VecM"))
    } else {
        None
    };

    let min_resource_fee = simulation
        .min_resource_fee
        .as_ref()
        .unwrap()
        .parse::<u32>()
        .unwrap_or(0);

    let soroban_tx_data = simulation
        .to_transaction_data()
        .expect("No transaction data");
    // Create a transaction builder with updated fees and Soroban data
    let source_acc = Rc::new(RefCell::new(
        Account::new(
            &tx.source.ok_or(Error::AccountNotFound)?,
            &tx.sequence.ok_or(Error::AccountNotFound)?,
        )
        .expect("Failed to copy source account data"),
    ));

    let mut tx_builder =
        TransactionBuilder::new(source_acc.clone(), network_passphrase, tx.time_bounds);

    tx_builder
        .fee(classic_fee_num + min_resource_fee)
        .set_soroban_data(soroban_tx_data);

    // Process the operation
    if let Some(ops) = tx.operations {
        if let OperationBody::InvokeHostFunction(invoke_host_function_op) = ops[0].clone().body {
            tx_builder.add_operation(
                stellar_baselib::operation::Operation::invoke_host_function(
                    invoke_host_function_op.host_function,
                    auth,
                    None,
                )
                .map_err(|_| Error::TransactionError)?,
            );
        }
    }

    Ok(tx_builder)
}

fn is_soroban_transaction(tx: &Transaction) -> bool {
    if let Some(operations) = &tx.operations {
        if operations.len() == 1 {
            let op = &operations[0];
            let valid = matches!(
                op.body.discriminant(),
                OperationType::InvokeHostFunction
                    | OperationType::ExtendFootprintTtl
                    | OperationType::RestoreFootprint
            );
            return valid;
        }
    }
    false
}

#[derive(Clone, Debug)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc, str::FromStr};

    use serde_json::json;
    use stellar_baselib::{
        account::{Account, AccountBehavior},
        transaction_builder::{TransactionBuilder, TransactionBuilderBehavior},
        xdr::{
            AccountId, CreateAccountOp, Hash, HostFunction, InvokeContractArgs,
            InvokeHostFunctionOp, Operation, OperationBody, PublicKey, ScAddress, ScSymbol, ScVal,
            SorobanAuthorizationEntry, StringM, Uint256, VecM,
        },
    };

    use crate::{error::Error, transaction::{
        assemble_transaction, is_soroban_transaction, SimulateTransactionResponse,
    }};

    #[test]
    fn simulation_failed() {

        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
                "0",
            )
            .unwrap(),
        ));
        let network = "Network for tests";

        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(Hash([0; 32])),
                    function_name: ScSymbol::from(StringM::from_str("test").unwrap()),
                    args: VecM::<ScVal>::try_from(Vec::new()).unwrap(),
                }),
                auth: VecM::<SorobanAuthorizationEntry>::try_from(Vec::new()).unwrap(),
            }),
        };

        let mut builder = TransactionBuilder::new(source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        builder.add_operation(op);
        let tx = builder.build();
        let simulation: SimulateTransactionResponse = serde_json::from_value(json!(
         {
            "error": "This is an error",
            "latestLedger": 2552139
          }
        
        )).unwrap();

        let r = assemble_transaction(tx, network, simulation);
        assert!(matches!(r, Err(Error::SimulationFailed)));
    }

    #[test]
    fn is_soroban_transaction_false() {
        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
                "0",
            )
            .unwrap(),
        ));
        let network = "Network for tests";

        let op = Operation {
            source_account: None,
            body: OperationBody::CreateAccount(CreateAccountOp {
                destination: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0; 32]))),
                starting_balance: 10,
            }),
        };

        let mut builder = TransactionBuilder::new(source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        builder.add_operation(op);
        let tx = builder.build();

        assert!(
            !is_soroban_transaction(&tx),
            "CreateAccountOp is not a soroban op"
        );

        let simulation: SimulateTransactionResponse = serde_json::from_value(json!(
         {
            "transactionData": "AAAAAAAAAAIAAAAGAAAAAcwD/nT9D7Dc2LxRdab+2vEUF8B+XoN7mQW21oxPT8ALAAAAFAAAAAEAAAAHy8vNUZ8vyZ2ybPHW0XbSrRtP7gEWsJ6zDzcfY9P8z88AAAABAAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAEAHfKyAAAFiAAAAIgAAAAAAAAAAw==",
            "minResourceFee": "90353",
            "events": [
              "AAAAAQAAAAAAAAAAAAAAAgAAAAAAAAADAAAADwAAAAdmbl9jYWxsAAAAAA0AAAAgzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAAPAAAACWluY3JlbWVudAAAAAAAABAAAAABAAAAAgAAABIAAAAAAAAAACDh1sDGwYAYgJ8EbeJPZwoZhDqEriwlbNnqivULm/oYAAAAAwAAAAM=",
              "AAAAAQAAAAAAAAABzAP+dP0PsNzYvFF1pv7a8RQXwH5eg3uZBbbWjE9PwAsAAAACAAAAAAAAAAIAAAAPAAAACWZuX3JldHVybgAAAAAAAA8AAAAJaW5jcmVtZW50AAAAAAAAAwAAAAw="
            ],
            "results": [
              {
                "auth": [],
                "xdr": "AAAAAwAAAAw="
              }
            ],
            "cost": {
              "cpuInsns": "1635562",
              "memBytes": "1295756"
            },
            "latestLedger": 2552139
          }
        
        )).unwrap();

        let r = assemble_transaction(tx, network, simulation);
        assert!(matches!(r, Err(Error::InvalidSorobanTransaction)));
    }

    #[test]
    fn is_soroban_transaction_true() {
        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
                "0",
            )
            .unwrap(),
        ));
        let network = "Network for tests";

        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(Hash([0; 32])),
                    function_name: ScSymbol::from(StringM::from_str("test").unwrap()),
                    args: VecM::<ScVal>::try_from(Vec::new()).unwrap(),
                }),
                auth: VecM::<SorobanAuthorizationEntry>::try_from(Vec::new()).unwrap(),
            }),
        };

        let mut builder = TransactionBuilder::new(source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        builder.add_operation(op);
        let tx = builder.build();

        assert!(
            is_soroban_transaction(&tx),
            "InvokeHostFunction is a soroban op"
        );
    }

    #[test]
    fn is_soroban_transaction_2_ops() {
        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
                "0",
            )
            .unwrap(),
        ));
        let network = "Network for tests";

        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(Hash([0; 32])),
                    function_name: ScSymbol::from(StringM::from_str("test").unwrap()),
                    args: VecM::<ScVal>::try_from(Vec::new()).unwrap(),
                }),
                auth: VecM::<SorobanAuthorizationEntry>::try_from(Vec::new()).unwrap(),
            }),
        };

        let mut builder = TransactionBuilder::new(source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        builder.add_operation(op.clone());
        builder.add_operation(op);
        let tx = builder.build();

        assert!(
            !is_soroban_transaction(&tx),
            "2 operations even InvokeHostFunction is not valid"
        );
    }
    #[test]
    fn is_soroban_transaction_no_ops() {
        let source_account = Rc::new(RefCell::new(
            Account::new(
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
                "0",
            )
            .unwrap(),
        ));
        let network = "Network for tests";

        let mut builder = TransactionBuilder::new(source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        let tx = builder.build();

        assert!(!is_soroban_transaction(&tx), "no ops is not valid");
    }
}
