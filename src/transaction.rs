use crate::{error::Error, soroban_rpc::*};
use stellar_baselib::operation::Operation;
pub use stellar_baselib::{
    account::Account,
    account::AccountBehavior,
    soroban_data_builder::{self, SorobanDataBuilder},
    transaction::Transaction,
    transaction::TransactionBehavior,
    transaction_builder::TransactionBuilder,
    transaction_builder::TransactionBuilderBehavior,
    xdr::{
        DiagnosticEvent, InvokeHostFunctionOp, Limits, OperationBody, OperationType, ReadXdr,
        ScVal, SorobanAuthorizationEntry, VecM,
    },
};

/// Assemble a [transaction](Transaction) with a [simulation](SimulateTransactionResponse)
pub fn assemble_transaction(
    tx: &Transaction,
    simulation: SimulateTransactionResponse,
) -> Result<Transaction, Error> {
    // Ensure the transaction is a valid Soroban transaction
    if !is_soroban_transaction(tx) {
        return Err(Error::InvalidSorobanTransaction);
    }

    if let Some(error) = simulation.error {
        return Err(Error::SimulationFailed(error));
    }

    if let Some((min_fee, restore)) = simulation.to_restore_transaction_data() {
        return Err(Error::RestorationRequired(min_fee, restore));
    }

    let sim_auth = if let Some((_, a)) = simulation.to_result() {
        Some(a)
    } else {
        None
    };

    let min_resource_fee = simulation
        .min_resource_fee
        .as_deref()
        .and_then(|fee| fee.parse::<u32>().ok())
        .ok_or_else(|| Error::JsonError("minResourceFee".to_string()))?;

    let soroban_tx_data = simulation
        .to_transaction_data()
        .expect("No transaction data");

    // A transaction that was already assembled carries the previous resource fee in its
    // fee. Take it out first, so assembling twice doesn't count the resource fee twice.
    let mut inclusion_fee = tx.fee;
    if let Some(data) = &tx.soroban_data {
        if let Ok(previous_resource_fee) = u32::try_from(data.resource_fee) {
            if inclusion_fee > previous_resource_fee {
                inclusion_fee -= previous_resource_fee;
            }
        }
    }

    let mut ntx = tx.clone();
    ntx.fee = inclusion_fee
        .checked_add(min_resource_fee)
        .ok_or(Error::TransactionError)?;
    ntx.soroban_data = Some(soroban_tx_data);

    // Process the operation
    if let Some(ops) = &tx.operations {
        if let OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function,
            auth,
        }) = ops[0].clone().body
        {
            let checked_auth = if auth.is_empty() {
                sim_auth
            } else {
                Some(auth.into())
            };
            ntx.operations = Some(vec![Operation::new()
                .invoke_host_function(host_function, checked_auth)
                .map_err(|_| Error::TransactionError)?]);
        }
    }

    Ok(ntx)
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

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use serde_json::json;
    use stellar_baselib::{
        account::{Account, AccountBehavior},
        transaction_builder::{TransactionBuilder, TransactionBuilderBehavior},
        xdr::{
            AccountId, ContractId, CreateAccountOp, Hash, HostFunction, InvokeContractArgs,
            InvokeHostFunctionOp, Operation, OperationBody, PublicKey, ScAddress, ScSymbol, ScVal,
            SorobanAuthorizationEntry, SorobanAuthorizedFunction, SorobanAuthorizedInvocation,
            SorobanCredentials, StringM, Uint256, VecM,
        },
    };

    use crate::{
        error::Error,
        transaction::{assemble_transaction, is_soroban_transaction, SimulateTransactionResponse},
    };

    #[test]
    fn tx_with_auth() {
        let mut source_account = Account::new(
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            "0",
        )
        .unwrap();
        let network = "Network for tests";

        let contract_address = ScAddress::Contract(ContractId(Hash([0; 32])));
        let function_name = ScSymbol::from(StringM::from_str("test").unwrap());
        let auth = SorobanAuthorizationEntry {
            credentials: SorobanCredentials::SourceAccount,
            root_invocation: SorobanAuthorizedInvocation {
                function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                    contract_address,
                    function_name,
                    args: VecM::<_>::try_from(Vec::new()).unwrap(),
                }),
                sub_invocations: VecM::<_>::try_from(Vec::new()).unwrap(),
            },
        };

        let contract_address = ScAddress::Contract(ContractId(Hash([0; 32])));
        let function_name = ScSymbol::from(StringM::from_str("test").unwrap());
        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address,
                    function_name,
                    args: VecM::<ScVal>::try_from(Vec::new()).unwrap(),
                }),
                auth: VecM::<SorobanAuthorizationEntry>::try_from(vec![auth]).unwrap(),
            }),
        };

        let mut builder = TransactionBuilder::new(&mut source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        builder.add_operation(op);
        let tx = builder.build();
        let simulation: SimulateTransactionResponse = serde_json::from_value(json!(
        {
            "transactionData": "AAAAAAAAAAIAAAAGAAAAAcwD/nT9D7Dc2LxRdab+2vEUF8B+XoN7mQW21oxPT8ALAAAAFAAAAAEAAAAHy8vNUZ8vyZ2ybPHW0XbSrRtP7gEWsJ6zDzcfY9P8z88AAAABAAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAEAHfKyAAAFiAAAAIgAAAAAAAAAAw==",
            "minResourceFee": "90353",
            "events": [
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

        let txr = assemble_transaction(&tx, simulation).unwrap();
        if let Some(ops) = txr.operations {
            let op = ops[0].clone();
            if let OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: _,
                auth,
            }) = op.body
            {
                assert_eq!(auth.len(), 1);
                assert!(matches!(
                    auth[0].credentials,
                    SorobanCredentials::SourceAccount
                ));
            } else {
                panic!("Failed")
            }
        }
    }

    #[test]
    fn simulation_failed() {
        let mut source_account = Account::new(
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            "0",
        )
        .unwrap();
        let network = "Network for tests";

        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(ContractId(Hash([0; 32]))),
                    function_name: ScSymbol::from(StringM::from_str("test").unwrap()),
                    args: VecM::<ScVal>::try_from(Vec::new()).unwrap(),
                }),
                auth: VecM::<SorobanAuthorizationEntry>::try_from(Vec::new()).unwrap(),
            }),
        };

        let mut builder = TransactionBuilder::new(&mut source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        builder.add_operation(op);
        let tx = builder.build();
        let simulation: SimulateTransactionResponse = serde_json::from_value(json!(
         {
            "error": "This is an error",
            "latestLedger": 2552139
          }

        ))
        .unwrap();

        let r = assemble_transaction(&tx, simulation);
        assert!(matches!(r, Err(Error::SimulationFailed(_))));
    }

    #[test]
    fn is_soroban_transaction_false() {
        let mut source_account = Account::new(
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            "0",
        )
        .unwrap();
        let network = "Network for tests";

        let op = Operation {
            source_account: None,
            body: OperationBody::CreateAccount(CreateAccountOp {
                destination: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0; 32]))),
                starting_balance: 10,
            }),
        };

        let mut builder = TransactionBuilder::new(&mut source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        builder.add_operation(op);
        let tx = builder.build();

        assert!(!is_soroban_transaction(&tx),);

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

        let r = assemble_transaction(&tx, simulation);
        assert!(matches!(r, Err(Error::InvalidSorobanTransaction)));
    }

    #[test]
    fn is_soroban_transaction_true() {
        let mut source_account = Account::new(
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            "0",
        )
        .unwrap();
        let network = "Network for tests";

        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(ContractId(Hash([0; 32]))),
                    function_name: ScSymbol::from(StringM::from_str("test").unwrap()),
                    args: VecM::<ScVal>::try_from(Vec::new()).unwrap(),
                }),
                auth: VecM::<SorobanAuthorizationEntry>::try_from(Vec::new()).unwrap(),
            }),
        };

        let mut builder = TransactionBuilder::new(&mut source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        builder.add_operation(op);
        let tx = builder.build();

        assert!(is_soroban_transaction(&tx),);
    }

    #[test]
    fn is_soroban_transaction_2_ops() {
        let mut source_account = Account::new(
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            "0",
        )
        .unwrap();
        let network = "Network for tests";

        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(ContractId(Hash([0; 32]))),
                    function_name: ScSymbol::from(StringM::from_str("test").unwrap()),
                    args: VecM::<ScVal>::try_from(Vec::new()).unwrap(),
                }),
                auth: VecM::<SorobanAuthorizationEntry>::try_from(Vec::new()).unwrap(),
            }),
        };

        let mut builder = TransactionBuilder::new(&mut source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        builder.add_operation(op.clone());
        builder.add_operation(op);
        let tx = builder.build();

        assert!(!is_soroban_transaction(&tx),);
    }
    #[test]
    fn is_soroban_transaction_no_ops() {
        let mut source_account = Account::new(
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            "0",
        )
        .unwrap();
        let network = "Network for tests";

        let mut builder = TransactionBuilder::new(&mut source_account, network, None);
        builder.fee(1000u32).set_timeout(30).unwrap();
        let tx = builder.build();

        assert!(!is_soroban_transaction(&tx),);
    }

    const TX_DATA: &str = "AAAAAAAAAAIAAAAGAAAAAcwD/nT9D7Dc2LxRdab+2vEUF8B+XoN7mQW21oxPT8ALAAAAFAAAAAEAAAAHy8vNUZ8vyZ2ybPHW0XbSrRtP7gEWsJ6zDzcfY9P8z88AAAABAAAABgAAAAHMA/50/Q+w3Ni8UXWm/trxFBfAfl6De5kFttaMT0/ACwAAABAAAAABAAAAAgAAAA8AAAAHQ291bnRlcgAAAAASAAAAAAAAAAAg4dbAxsGAGICfBG3iT2cKGYQ6hK4sJWzZ6or1C5v6GAAAAAEAHfKyAAAFiAAAAIgAAAAAAAAAAw==";

    fn invoke_tx(fee: u32) -> stellar_baselib::transaction::Transaction {
        let mut source_account = Account::new(
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            "0",
        )
        .unwrap();
        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(ContractId(Hash([0; 32]))),
                    function_name: ScSymbol::from(StringM::from_str("test").unwrap()),
                    args: VecM::<ScVal>::try_from(Vec::new()).unwrap(),
                }),
                auth: VecM::<SorobanAuthorizationEntry>::try_from(Vec::new()).unwrap(),
            }),
        };
        let mut builder = TransactionBuilder::new(&mut source_account, "Network for tests", None);
        builder.fee(fee).set_timeout(30).unwrap();
        builder.add_operation(op);
        builder.build()
    }

    /// A simulation whose transaction data carries `resource_fee`, as a real one does.
    fn simulation_with_resource_fee(resource_fee: i64) -> SimulateTransactionResponse {
        use stellar_baselib::xdr::{Limits, ReadXdr, SorobanTransactionData, WriteXdr};
        let mut data = SorobanTransactionData::from_xdr_base64(TX_DATA, Limits::none()).unwrap();
        data.resource_fee = resource_fee;
        serde_json::from_value(json!({
            "transactionData": data.to_xdr_base64(Limits::none()).unwrap(),
            "minResourceFee": resource_fee.to_string(),
            "events": [],
            "results": [{ "auth": [], "xdr": "AAAAAwAAAAw=" }],
            "cost": { "cpuInsns": "1635562", "memBytes": "1295756" },
            "latestLedger": 2552139
        }))
        .unwrap()
    }

    #[test]
    fn assembling_twice_counts_the_resource_fee_once() {
        // The numbers from the testnet reproduction in #34.
        let tx = invoke_tx(100);
        let once = assemble_transaction(&tx, simulation_with_resource_fee(7166)).unwrap();
        assert_eq!(once.fee, 7266);

        let twice = assemble_transaction(&once, simulation_with_resource_fee(7166)).unwrap();
        assert_eq!(twice.fee, 7266);
        assert_eq!(twice.soroban_data.unwrap().resource_fee, 7166);
    }

    #[test]
    fn reassembling_takes_the_new_resource_fee() {
        let tx = invoke_tx(100);
        let once = assemble_transaction(&tx, simulation_with_resource_fee(7166)).unwrap();
        let again = assemble_transaction(&once, simulation_with_resource_fee(9000)).unwrap();
        assert_eq!(again.fee, 9100);
    }

    #[test]
    fn missing_min_resource_fee_is_an_error() {
        let simulation: SimulateTransactionResponse = serde_json::from_value(json!({
            "transactionData": TX_DATA,
            "events": [],
            "results": [{ "auth": [], "xdr": "AAAAAwAAAAw=" }],
            "latestLedger": 2552139
        }))
        .unwrap();
        let r = assemble_transaction(&invoke_tx(100), simulation);
        assert!(matches!(r, Err(Error::JsonError(_))));
    }

    #[test]
    fn fee_overflow_is_an_error() {
        let r = assemble_transaction(
            &invoke_tx(u32::MAX - 10),
            simulation_with_resource_fee(7166),
        );
        assert!(matches!(r, Err(Error::TransactionError)));
    }
}
