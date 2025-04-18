use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU128;
use std::sync::Arc;

use blockifier::blockifier::block::{BlockInfo, GasPrices};
use blockifier::bouncer::{Bouncer, BouncerConfig};
use blockifier::context::{BlockContext, ChainInfo, FeeTokenAddresses};
use blockifier::execution::call_info::{
    CallExecution, CallInfo, OrderedEvent, OrderedL2ToL1Message,
};
use blockifier::execution::contract_class::{
    ClassInfo, ContractClass, ContractClassV0, ContractClassV1,
};
use blockifier::execution::entry_point::CallType;
use blockifier::fee::fee_utils::get_fee_by_gas_vector;
use blockifier::state::cached_state::{self, TransactionalState};
use blockifier::state::state_api::{StateReader, UpdatableState};
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::objects::{FeeType, HasRelatedFeeType, TransactionExecutionInfo};
use blockifier::transaction::transaction_execution::Transaction;
use blockifier::transaction::transactions::{
    DeclareTransaction, DeployAccountTransaction, ExecutableTransaction, InvokeTransaction,
    L1HandlerTransaction,
};
use blockifier::versioned_constants::{StarknetVersion, VersionedConstants};
use katana_cairo::cairo_vm::types::errors::program_errors::ProgramError;
use katana_cairo::cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use katana_cairo::starknet_api::block::{BlockNumber, BlockTimestamp};
use katana_cairo::starknet_api::core::{
    self, ChainId, ClassHash, CompiledClassHash, ContractAddress, EntryPointSelector, Nonce,
};
use katana_cairo::starknet_api::data_availability::DataAvailabilityMode;
use katana_cairo::starknet_api::deprecated_contract_class::EntryPointType;
use katana_cairo::starknet_api::transaction::{
    AccountDeploymentData, Calldata, ContractAddressSalt,
    DeclareTransaction as ApiDeclareTransaction, DeclareTransactionV0V1, DeclareTransactionV2,
    DeclareTransactionV3, DeployAccountTransaction as ApiDeployAccountTransaction,
    DeployAccountTransactionV1, DeployAccountTransactionV3, Fee,
    InvokeTransaction as ApiInvokeTransaction, PaymasterData, Resource, ResourceBounds,
    ResourceBoundsMapping, Tip, TransactionHash, TransactionSignature, TransactionVersion,
};
use katana_primitives::chain::NamedChainId;
use katana_primitives::env::{BlockEnv, CfgEnv};
use katana_primitives::fee::{PriceUnit, TxFeeInfo};
use katana_primitives::state::{StateUpdates, StateUpdatesWithClasses};
use katana_primitives::trace::{L1Gas, TxExecInfo, TxResources};
use katana_primitives::transaction::{
    DeclareTx, DeployAccountTx, ExecutableTx, ExecutableTxWithHash, InvokeTx, TxType,
};
use katana_primitives::{class, event, message, trace};
use katana_provider::traits::contract::ContractClassProvider;
use starknet::core::utils::parse_cairo_short_string;

use super::state::CachedState;
use crate::abstraction::ExecutionFlags;
use crate::utils::build_receipt;
use crate::{ExecutionError, ExecutionResult, ExecutorResult};

pub fn transact<S: StateReader>(
    state: &mut cached_state::CachedState<S>,
    block_context: &BlockContext,
    simulation_flags: &ExecutionFlags,
    tx: ExecutableTxWithHash,
    bouncer: Option<&mut Bouncer>,
) -> ExecutorResult<ExecutionResult> {
    fn transact_inner<U: UpdatableState>(
        state: &mut U,
        block_context: &BlockContext,
        simulation_flags: &ExecutionFlags,
        tx: Transaction,
    ) -> Result<(TransactionExecutionInfo, TxFeeInfo), ExecutionError> {
        let validate = simulation_flags.account_validation();
        let charge_fee = simulation_flags.fee();
        // Blockifier doesn't provide a way to fully skip nonce check during the tx validation
        // stage. The `nonce_check` flag in `tx.execute()` only 'relaxes' the check for
        // nonce that is equal or higher than the current (expected) account nonce.
        //
        // Related commit on Blockifier: https://github.com/dojoengine/blockifier/commit/2410b6055453f247d48759f223c34b3fb5fa777
        let nonce_check = simulation_flags.nonce_check();

        let fee_type = get_fee_type_from_tx(&tx);
        let info = match tx {
            Transaction::AccountTransaction(tx) => {
                tx.execute(state, block_context, charge_fee, validate, nonce_check)
            }
            Transaction::L1HandlerTransaction(tx) => {
                tx.execute(state, block_context, charge_fee, validate, nonce_check)
            }
        }?;

        // There are a few case where the `actual_fee` field of the transaction info is not set
        // where the fee is skipped and thus not charged for the transaction (e.g. when the
        // `skip_fee_transfer` is explicitly set, or when the transaction `max_fee` is set to 0). In
        // these cases, we still want to calculate the fee.
        let fee = if info.transaction_receipt.fee == Fee(0) {
            get_fee_by_gas_vector(
                block_context.block_info(),
                info.transaction_receipt.gas,
                &fee_type,
            )
        } else {
            info.transaction_receipt.fee
        };

        let gas_consumed = info.transaction_receipt.gas.l1_gas;

        let (unit, gas_price) = match fee_type {
            FeeType::Eth => {
                (PriceUnit::Wei, block_context.block_info().gas_prices.eth_l1_gas_price)
            }
            FeeType::Strk => {
                (PriceUnit::Fri, block_context.block_info().gas_prices.strk_l1_gas_price)
            }
        };

        let fee_info =
            TxFeeInfo { gas_consumed, gas_price: gas_price.into(), unit, overall_fee: fee.0 };

        Ok((info, fee_info))
    }

    let transaction = to_executor_tx(tx.clone());
    let mut tx_state = TransactionalState::create_transactional(state);
    let result = transact_inner(&mut tx_state, block_context, simulation_flags, transaction);

    match result {
        Ok((info, fee)) => {
            if let Some(bouncer) = bouncer {
                let tx_state_changes_keys =
                    tx_state.get_actual_state_changes().unwrap().into_keys();

                bouncer.try_update(
                    &tx_state,
                    &tx_state_changes_keys,
                    &info.summarize(),
                    &info.transaction_receipt.resources,
                )?;
            }

            tx_state.commit();

            // get the trace and receipt from the execution info
            let trace = to_exec_info(info, tx.r#type());
            let receipt = build_receipt(tx.tx_ref(), fee, &trace);
            Ok(ExecutionResult::new_success(receipt, trace))
        }

        Err(e) => {
            tx_state.commit();
            Ok(ExecutionResult::new_failed(e))
        }
    }
}

pub fn to_executor_tx(tx: ExecutableTxWithHash) -> Transaction {
    let hash = tx.hash;

    match tx.transaction {
        ExecutableTx::Invoke(tx) => match tx {
            InvokeTx::V0(tx) => {
                let calldata = tx.calldata;
                let signature = tx.signature;

                Transaction::AccountTransaction(AccountTransaction::Invoke(InvokeTransaction {
                    tx: ApiInvokeTransaction::V0(
                        katana_cairo::starknet_api::transaction::InvokeTransactionV0 {
                            entry_point_selector: EntryPointSelector(tx.entry_point_selector),
                            contract_address: to_blk_address(tx.contract_address),
                            signature: TransactionSignature(signature),
                            calldata: Calldata(Arc::new(calldata)),
                            max_fee: Fee(tx.max_fee),
                        },
                    ),
                    tx_hash: TransactionHash(hash),
                    only_query: false,
                }))
            }

            InvokeTx::V1(tx) => {
                let calldata = tx.calldata;
                let signature = tx.signature;

                Transaction::AccountTransaction(AccountTransaction::Invoke(InvokeTransaction {
                    tx: ApiInvokeTransaction::V1(
                        katana_cairo::starknet_api::transaction::InvokeTransactionV1 {
                            max_fee: Fee(tx.max_fee),
                            nonce: Nonce(tx.nonce),
                            sender_address: to_blk_address(tx.sender_address),
                            signature: TransactionSignature(signature),
                            calldata: Calldata(Arc::new(calldata)),
                        },
                    ),
                    tx_hash: TransactionHash(hash),
                    only_query: false,
                }))
            }

            InvokeTx::V3(tx) => {
                let calldata = tx.calldata;
                let signature = tx.signature;

                let paymaster_data = tx.paymaster_data;
                let account_deploy_data = tx.account_deployment_data;
                let fee_data_availability_mode = to_api_da_mode(tx.fee_data_availability_mode);
                let nonce_data_availability_mode = to_api_da_mode(tx.nonce_data_availability_mode);

                Transaction::AccountTransaction(AccountTransaction::Invoke(InvokeTransaction {
                    tx: ApiInvokeTransaction::V3(
                        katana_cairo::starknet_api::transaction::InvokeTransactionV3 {
                            tip: Tip(tx.tip),
                            nonce: Nonce(tx.nonce),
                            sender_address: to_blk_address(tx.sender_address),
                            signature: TransactionSignature(signature),
                            calldata: Calldata(Arc::new(calldata)),
                            paymaster_data: PaymasterData(paymaster_data),
                            account_deployment_data: AccountDeploymentData(account_deploy_data),
                            fee_data_availability_mode,
                            nonce_data_availability_mode,
                            resource_bounds: to_api_resource_bounds(tx.resource_bounds),
                        },
                    ),
                    tx_hash: TransactionHash(hash),
                    only_query: false,
                }))
            }
        },

        ExecutableTx::DeployAccount(tx) => match tx {
            DeployAccountTx::V1(tx) => {
                let calldata = tx.constructor_calldata;
                let signature = tx.signature;
                let salt = ContractAddressSalt(tx.contract_address_salt);

                Transaction::AccountTransaction(AccountTransaction::DeployAccount(
                    DeployAccountTransaction {
                        contract_address: to_blk_address(tx.contract_address),
                        tx: ApiDeployAccountTransaction::V1(DeployAccountTransactionV1 {
                            max_fee: Fee(tx.max_fee),
                            nonce: Nonce(tx.nonce),
                            signature: TransactionSignature(signature),
                            class_hash: ClassHash(tx.class_hash),
                            constructor_calldata: Calldata(Arc::new(calldata)),
                            contract_address_salt: salt,
                        }),
                        tx_hash: TransactionHash(hash),
                        only_query: false,
                    },
                ))
            }

            DeployAccountTx::V3(tx) => {
                let calldata = tx.constructor_calldata;
                let signature = tx.signature;
                let salt = ContractAddressSalt(tx.contract_address_salt);

                let paymaster_data = tx.paymaster_data;
                let fee_data_availability_mode = to_api_da_mode(tx.fee_data_availability_mode);
                let nonce_data_availability_mode = to_api_da_mode(tx.nonce_data_availability_mode);

                Transaction::AccountTransaction(AccountTransaction::DeployAccount(
                    DeployAccountTransaction {
                        contract_address: to_blk_address(tx.contract_address),
                        tx: ApiDeployAccountTransaction::V3(DeployAccountTransactionV3 {
                            tip: Tip(tx.tip),
                            nonce: Nonce(tx.nonce),
                            signature: TransactionSignature(signature),
                            class_hash: ClassHash(tx.class_hash),
                            constructor_calldata: Calldata(Arc::new(calldata)),
                            contract_address_salt: salt,
                            paymaster_data: PaymasterData(paymaster_data),
                            fee_data_availability_mode,
                            nonce_data_availability_mode,
                            resource_bounds: to_api_resource_bounds(tx.resource_bounds),
                        }),
                        tx_hash: TransactionHash(hash),
                        only_query: false,
                    },
                ))
            }
        },

        ExecutableTx::Declare(tx) => {
            let compiled = tx.class.as_ref().clone().compile().expect("failed to compile");

            let tx = match tx.transaction {
                DeclareTx::V0(tx) => ApiDeclareTransaction::V0(DeclareTransactionV0V1 {
                    max_fee: Fee(tx.max_fee),
                    nonce: Nonce::default(),
                    sender_address: to_blk_address(tx.sender_address),
                    signature: TransactionSignature(tx.signature),
                    class_hash: ClassHash(tx.class_hash),
                }),

                DeclareTx::V1(tx) => ApiDeclareTransaction::V1(DeclareTransactionV0V1 {
                    max_fee: Fee(tx.max_fee),
                    nonce: Nonce(tx.nonce),
                    sender_address: to_blk_address(tx.sender_address),
                    signature: TransactionSignature(tx.signature),
                    class_hash: ClassHash(tx.class_hash),
                }),

                DeclareTx::V2(tx) => {
                    let signature = tx.signature;

                    ApiDeclareTransaction::V2(DeclareTransactionV2 {
                        max_fee: Fee(tx.max_fee),
                        nonce: Nonce(tx.nonce),
                        sender_address: to_blk_address(tx.sender_address),
                        signature: TransactionSignature(signature),
                        class_hash: ClassHash(tx.class_hash),
                        compiled_class_hash: CompiledClassHash(tx.compiled_class_hash),
                    })
                }

                DeclareTx::V3(tx) => {
                    let signature = tx.signature;

                    let paymaster_data = tx.paymaster_data;
                    let fee_data_availability_mode = to_api_da_mode(tx.fee_data_availability_mode);
                    let nonce_data_availability_mode =
                        to_api_da_mode(tx.nonce_data_availability_mode);
                    let account_deploy_data = tx.account_deployment_data;

                    ApiDeclareTransaction::V3(DeclareTransactionV3 {
                        tip: Tip(tx.tip),
                        nonce: Nonce(tx.nonce),
                        sender_address: to_blk_address(tx.sender_address),
                        signature: TransactionSignature(signature),
                        class_hash: ClassHash(tx.class_hash),
                        account_deployment_data: AccountDeploymentData(account_deploy_data),
                        compiled_class_hash: CompiledClassHash(tx.compiled_class_hash),
                        paymaster_data: PaymasterData(paymaster_data),
                        fee_data_availability_mode,
                        nonce_data_availability_mode,
                        resource_bounds: to_api_resource_bounds(tx.resource_bounds),
                    })
                }
            };

            let hash = TransactionHash(hash);
            let class = to_class_info(compiled).unwrap();
            let tx = DeclareTransaction::new(tx, hash, class).expect("class mismatch");
            Transaction::AccountTransaction(AccountTransaction::Declare(tx))
        }

        ExecutableTx::L1Handler(tx) => Transaction::L1HandlerTransaction(L1HandlerTransaction {
            paid_fee_on_l1: Fee(tx.paid_fee_on_l1),
            tx: katana_cairo::starknet_api::transaction::L1HandlerTransaction {
                nonce: core::Nonce(tx.nonce),
                calldata: Calldata(Arc::new(tx.calldata)),
                version: TransactionVersion(1u128.into()),
                contract_address: to_blk_address(tx.contract_address),
                entry_point_selector: core::EntryPointSelector(tx.entry_point_selector),
            },
            tx_hash: TransactionHash(hash),
        }),
    }
}

/// Create a block context from the chain environment values.
pub fn block_context_from_envs(block_env: &BlockEnv, cfg_env: &CfgEnv) -> BlockContext {
    let fee_token_addresses = FeeTokenAddresses {
        eth_fee_token_address: to_blk_address(cfg_env.fee_token_addresses.eth),
        strk_fee_token_address: to_blk_address(cfg_env.fee_token_addresses.strk),
    };

    let eth_l1_gas_price =
        NonZeroU128::new(block_env.l1_gas_prices.eth).unwrap_or(NonZeroU128::new(1).unwrap());
    let strk_l1_gas_price =
        NonZeroU128::new(block_env.l1_gas_prices.strk).unwrap_or(NonZeroU128::new(1).unwrap());
    let strk_l1_data_gas_price =
        NonZeroU128::new(block_env.l1_data_gas_prices.strk).unwrap_or(NonZeroU128::new(1).unwrap());
    let eth_l1_data_gas_price =
        NonZeroU128::new(block_env.l1_data_gas_prices.eth).unwrap_or(NonZeroU128::new(1).unwrap());

    let gas_prices = GasPrices {
        eth_l1_gas_price,
        strk_l1_gas_price,
        eth_l1_data_gas_price,
        strk_l1_data_gas_price,
    };

    let block_info = BlockInfo {
        block_number: BlockNumber(block_env.number),
        block_timestamp: BlockTimestamp(block_env.timestamp),
        sequencer_address: to_blk_address(block_env.sequencer_address),
        gas_prices,
        use_kzg_da: false,
    };

    let chain_info = ChainInfo { fee_token_addresses, chain_id: to_blk_chain_id(cfg_env.chain_id) };

    // IMPORTANT:
    //
    // The versioned constants that we use here must match the version that is used in `snos`.
    // Otherwise, there might be a mismatch between the calculated fees.
    //
    // The version of `snos` we're using is still limited up to Starknet version `0.13.3`.
    const SN_VERSION: StarknetVersion = StarknetVersion::Latest; // v0.13.3
    let mut versioned_constants = VersionedConstants::get(SN_VERSION).clone();

    // NOTE:
    // These overrides would potentially make the `snos` run be invalid as it doesn't know about the
    // new overridden values.
    versioned_constants.max_recursion_depth = cfg_env.max_recursion_depth;
    versioned_constants.validate_max_n_steps = cfg_env.validate_max_n_steps;
    versioned_constants.invoke_tx_max_n_steps = cfg_env.invoke_tx_max_n_steps;

    BlockContext::new(block_info, chain_info, versioned_constants, BouncerConfig::max())
}

pub(super) fn state_update_from_cached_state(state: &CachedState<'_>) -> StateUpdatesWithClasses {
    let state_diff = state.inner.lock().cached_state.to_state_diff().unwrap();

    let mut declared_contract_classes: BTreeMap<
        katana_primitives::class::ClassHash,
        katana_primitives::class::ContractClass,
    > = BTreeMap::new();

    let mut declared_classes = BTreeMap::new();
    let mut deprecated_declared_classes = BTreeSet::new();

    // TODO: Legacy class shouldn't have a compiled class hash. This is a hack we added
    // in our fork of `blockifier. Check if it's possible to remove it now.
    for (class_hash, compiled_hash) in state_diff.compiled_class_hashes {
        let hash = class_hash.0;
        let class = state.class(hash).unwrap().expect("must exist if declared");

        if class.is_legacy() {
            deprecated_declared_classes.insert(hash);
        } else {
            declared_classes.insert(hash, compiled_hash.0);
        }

        declared_contract_classes.insert(hash, class);
    }

    let nonce_updates =
        state_diff
            .nonces
            .into_iter()
            .map(|(key, value)| (to_address(key), value.0))
            .collect::<BTreeMap<
                katana_primitives::contract::ContractAddress,
                katana_primitives::contract::Nonce,
            >>();

    let storage_updates = state_diff.storage.into_iter().fold(
        BTreeMap::new(),
        |mut storage, ((addr, key), value)| {
            let entry: &mut BTreeMap<
                katana_primitives::contract::StorageKey,
                katana_primitives::contract::StorageValue,
            > = storage.entry(to_address(addr)).or_default();
            entry.insert(*key.0.key(), value);
            storage
        },
    );

    let deployed_contracts =
        state_diff
            .class_hashes
            .into_iter()
            .map(|(key, value)| (to_address(key), value.0))
            .collect::<BTreeMap<
                katana_primitives::contract::ContractAddress,
                katana_primitives::class::ClassHash,
            >>();

    StateUpdatesWithClasses {
        classes: declared_contract_classes,
        state_updates: StateUpdates {
            nonce_updates,
            storage_updates,
            declared_classes,
            deployed_contracts,
            deprecated_declared_classes,
            replaced_classes: BTreeMap::default(),
        },
    }
}

fn to_api_da_mode(mode: katana_primitives::da::DataAvailabilityMode) -> DataAvailabilityMode {
    match mode {
        katana_primitives::da::DataAvailabilityMode::L1 => DataAvailabilityMode::L1,
        katana_primitives::da::DataAvailabilityMode::L2 => DataAvailabilityMode::L2,
    }
}

fn to_api_resource_bounds(
    resource_bounds: katana_primitives::fee::ResourceBoundsMapping,
) -> ResourceBoundsMapping {
    let l1_gas = ResourceBounds {
        max_amount: resource_bounds.l1_gas.max_amount,
        max_price_per_unit: resource_bounds.l1_gas.max_price_per_unit,
    };

    let l2_gas = ResourceBounds {
        max_amount: resource_bounds.l2_gas.max_amount,
        max_price_per_unit: resource_bounds.l2_gas.max_price_per_unit,
    };

    ResourceBoundsMapping(BTreeMap::from([(Resource::L1Gas, l1_gas), (Resource::L2Gas, l2_gas)]))
}

/// Get the fee type of a transaction. The fee type determines the token used to pay for the
/// transaction.
fn get_fee_type_from_tx(transaction: &Transaction) -> FeeType {
    match transaction {
        Transaction::AccountTransaction(tx) => tx.fee_type(),
        Transaction::L1HandlerTransaction(tx) => tx.fee_type(),
    }
}

pub fn to_blk_address(address: katana_primitives::contract::ContractAddress) -> ContractAddress {
    address.0.try_into().expect("valid address")
}

pub fn to_address(address: ContractAddress) -> katana_primitives::contract::ContractAddress {
    katana_primitives::contract::ContractAddress(*address.0.key())
}

pub fn to_blk_chain_id(chain_id: katana_primitives::chain::ChainId) -> ChainId {
    match chain_id {
        katana_primitives::chain::ChainId::Named(NamedChainId::Mainnet) => ChainId::Mainnet,
        katana_primitives::chain::ChainId::Named(NamedChainId::Sepolia) => ChainId::Sepolia,
        katana_primitives::chain::ChainId::Named(named) => ChainId::Other(named.to_string()),
        katana_primitives::chain::ChainId::Id(id) => {
            let id = parse_cairo_short_string(&id).expect("valid cairo string");
            ChainId::Other(id)
        }
    }
}

pub fn to_class_info(class: class::CompiledClass) -> Result<ClassInfo, ProgramError> {
    // TODO: @kariy not sure of the variant that must be used in this case. Should we change the
    // return type to include this case of error for contract class conversions?
    match class {
        class::CompiledClass::Legacy(..) => {
            // For cairo 0, the sierra_program_length must be 0.
            Ok(ClassInfo::new(&to_class(class)?, 0, 0).unwrap())
        }

        class::CompiledClass::Class(..) => {
            // NOTE:
            //
            // Right now, we're using dummy values for the sierra class info (ie
            // sierra_program_length, and abi_length). This value affects the fee
            // calculation so we should use the correct values based on the sierra class itself.
            //
            // Make sure these values are the same over on `snos` when it re-executes the
            // transactions as otherwise the fees would be different.

            let class = to_class(class)?;
            let sierra_program_length = 1;
            let abi_length = 0;

            let class = ClassInfo::new(&class, sierra_program_length, abi_length).unwrap();
            Ok(class)
        }
    }
}

/// Convert katana-primitives compiled class to blockfiier's contract class.
pub fn to_class(class: class::CompiledClass) -> Result<ContractClass, ProgramError> {
    match class {
        class::CompiledClass::Legacy(class) => {
            Ok(ContractClass::V0(ContractClassV0::try_from(class)?))
        }
        class::CompiledClass::Class(casm) => {
            Ok(ContractClass::V1(ContractClassV1::try_from(casm)?))
        }
    }
}

pub fn to_exec_info(exec_info: TransactionExecutionInfo, r#type: TxType) -> TxExecInfo {
    TxExecInfo {
        r#type,
        validate_call_info: exec_info.validate_call_info.map(to_call_info),
        execute_call_info: exec_info.execute_call_info.map(to_call_info),
        fee_transfer_call_info: exec_info.fee_transfer_call_info.map(to_call_info),
        actual_fee: exec_info.transaction_receipt.fee.0,
        revert_error: exec_info.revert_error.clone(),
        actual_resources: TxResources {
            vm_resources: to_execution_resources(
                exec_info.transaction_receipt.resources.vm_resources,
            ),
            n_reverted_steps: exec_info.transaction_receipt.resources.n_reverted_steps,
            data_availability: L1Gas {
                l1_gas: exec_info.transaction_receipt.da_gas.l1_gas,
                l1_data_gas: exec_info.transaction_receipt.da_gas.l1_data_gas,
            },
            total_gas_consumed: L1Gas {
                l1_gas: exec_info.transaction_receipt.gas.l1_gas,
                l1_data_gas: exec_info.transaction_receipt.gas.l1_data_gas,
            },
        },
    }
}

fn to_call_info(call: CallInfo) -> trace::CallInfo {
    let contract_address = to_address(call.call.storage_address);
    let caller_address = to_address(call.call.caller_address);
    let code_address = call.call.code_address.map(to_address);
    let class_hash = call.call.class_hash.map(|a| a.0);
    let entry_point_selector = call.call.entry_point_selector.0;
    let calldata = call.call.calldata.0.as_ref().clone();
    let retdata = call.execution.retdata.0;

    let CallExecution { events, l2_to_l1_messages, .. } = call.execution;

    let events = events.into_iter().map(to_ordered_event).collect();
    let l1_msg =
        l2_to_l1_messages.into_iter().map(|m| to_l2_l1_messages(m, contract_address)).collect();

    let call_type = match call.call.call_type {
        CallType::Call => trace::CallType::Call,
        CallType::Delegate => trace::CallType::Delegate,
    };

    let entry_point_type = match call.call.entry_point_type {
        EntryPointType::External => trace::EntryPointType::External,
        EntryPointType::L1Handler => trace::EntryPointType::L1Handler,
        EntryPointType::Constructor => trace::EntryPointType::Constructor,
    };

    let storage_read_values = call.storage_read_values;
    let storg_keys = call.accessed_storage_keys.into_iter().map(|k| *k.0.key()).collect();
    let inner_calls = call.inner_calls.into_iter().map(to_call_info).collect();

    trace::CallInfo {
        contract_address,
        caller_address,
        call_type,
        code_address,
        class_hash,
        entry_point_selector,
        entry_point_type,
        calldata,
        retdata,
        execution_resources: to_execution_resources(call.resources),
        events,
        l2_to_l1_messages: l1_msg,
        storage_read_values,
        accessed_storage_keys: storg_keys,
        inner_calls,
        gas_consumed: call.execution.gas_consumed as u128,
        failed: call.execution.failed,
    }
}

fn to_ordered_event(e: OrderedEvent) -> event::OrderedEvent {
    event::OrderedEvent {
        order: e.order as u64,
        data: e.event.data.0,
        keys: e.event.keys.iter().map(|f| f.0).collect(),
    }
}

fn to_l2_l1_messages(
    m: OrderedL2ToL1Message,
    from_address: katana_primitives::contract::ContractAddress,
) -> message::OrderedL2ToL1Message {
    let order = m.order as u64;
    let payload = m.message.payload.0;
    let to_address = m.message.to_address;
    message::OrderedL2ToL1Message { order, from_address, to_address, payload }
}

fn to_execution_resources(
    resources: ExecutionResources,
) -> katana_primitives::trace::ExecutionResources {
    katana_primitives::trace::ExecutionResources {
        n_steps: resources.n_steps,
        n_memory_holes: resources.n_memory_holes,
        builtin_instance_counter: resources.builtin_instance_counter.into(),
    }
}

#[cfg(test)]
mod tests {

    use std::collections::{HashMap, HashSet};

    use blockifier::execution::entry_point::CallEntryPoint;
    use katana_cairo::cairo_vm::types::builtin_name::BuiltinName;
    use katana_cairo::cairo_vm::vm::runners::cairo_runner::ExecutionResources;
    use katana_cairo::starknet_api::core::EntryPointSelector;
    use katana_cairo::starknet_api::felt;
    use katana_cairo::starknet_api::transaction::{EventContent, EventData, EventKey};
    use katana_primitives::Felt;

    use super::*;

    #[test]
    fn convert_chain_id() {
        let katana_mainnet = katana_primitives::chain::ChainId::MAINNET;
        let katana_sepolia = katana_primitives::chain::ChainId::SEPOLIA;
        let katana_id = katana_primitives::chain::ChainId::Id(felt!("0x1337"));

        let blockifier_mainnet = to_blk_chain_id(katana_mainnet);
        let blockifier_sepolia = to_blk_chain_id(katana_sepolia);
        let blockifier_id = to_blk_chain_id(katana_id);

        assert_eq!(blockifier_mainnet, ChainId::Mainnet);
        assert_eq!(blockifier_sepolia, ChainId::Sepolia);
        assert_eq!(blockifier_id.as_hex(), katana_id.to_string());
    }

    /// Test to ensure that when Blockifier pass the chain id to the contract ( thru a syscall eg,
    /// get_tx_inbox().unbox().chain_id ), the value is exactly the same as Katana chain id.
    ///
    /// Issue: <https://github.com/dojoengine/dojo/issues/1595>
    #[test]
    fn blockifier_chain_id_invariant() {
        let id = felt!("0x1337");

        let katana_id = katana_primitives::chain::ChainId::Id(id);
        let blockifier_id = to_blk_chain_id(katana_id);

        // Mimic how blockifier convert from ChainId to FieldElement.
        //
        // This is how blockifier pass the chain id to the contract through a syscall.
        // https://github.com/dojoengine/blockifier/blob/f2246ce2862d043e4efe2ecf149a4cb7bee689cd/crates/blockifier/src/execution/syscalls/hint_processor.rs#L600-L602
        let actual_id = Felt::from_hex(blockifier_id.as_hex().as_str()).unwrap();

        assert_eq!(actual_id, id)
    }

    fn create_blockifier_call_info() -> CallInfo {
        let top_events = vec![OrderedEvent {
            order: 0,
            event: EventContent {
                data: EventData(vec![888u128.into()]),
                keys: vec![EventKey(999u128.into())],
            },
        }];
        let nested_events = vec![
            OrderedEvent {
                order: 1,
                event: EventContent {
                    data: EventData(vec![889u128.into()]),
                    keys: vec![EventKey(990u128.into())],
                },
            },
            OrderedEvent {
                order: 2,
                event: EventContent {
                    data: EventData(vec![0u128.into()]),
                    keys: vec![EventKey(9u128.into())],
                },
            },
        ];

        let nested_call = CallInfo {
            execution: CallExecution { events: nested_events, ..Default::default() },
            ..Default::default()
        };

        CallInfo {
            call: CallEntryPoint {
                class_hash: None,
                initial_gas: 77,
                call_type: CallType::Call,
                caller_address: 200u128.into(),
                storage_address: 100u128.into(),
                code_address: Some(100u128.into()),
                entry_point_type: EntryPointType::External,
                calldata: Calldata(Arc::new(vec![felt!(1_u8)])),
                entry_point_selector: EntryPointSelector(felt!(999_u32)),
            },
            execution: CallExecution {
                failed: true,
                gas_consumed: 12345,
                events: top_events,
                ..Default::default()
            },
            storage_read_values: vec![felt!(1_u8), felt!(2_u8)],
            accessed_storage_keys: HashSet::from([3u128.into(), 4u128.into(), 5u128.into()]),
            resources: ExecutionResources {
                n_steps: 1_000_000,
                n_memory_holes: 9_000,
                builtin_instance_counter: HashMap::from([
                    (BuiltinName::ecdsa, 50),
                    (BuiltinName::pedersen, 9),
                ]),
            },
            inner_calls: vec![nested_call],
        }
    }

    #[test]
    fn convert_call_info() {
        // setup expected values
        let call = create_blockifier_call_info();

        let expected_contract_address = to_address(call.call.storage_address);
        let expected_caller_address = to_address(call.call.caller_address);
        let expected_code_address = call.call.code_address.map(to_address);
        let expected_class_hash = call.call.class_hash.map(|c| c.0);
        let expected_entry_point_selector = call.call.entry_point_selector.0;
        let expected_calldata = call.call.calldata.0.as_ref().clone();
        let expected_retdata = call.execution.retdata.0.clone();

        let CallExecution { events, l2_to_l1_messages, .. } = call.execution.clone();
        let expected_events: Vec<_> = events.into_iter().map(to_ordered_event).collect();
        let expected_l2_to_l1_msg: Vec<_> = l2_to_l1_messages
            .into_iter()
            .map(|m| to_l2_l1_messages(m, expected_contract_address))
            .collect();

        let expected_call_type = match call.call.call_type {
            CallType::Call => trace::CallType::Call,
            CallType::Delegate => trace::CallType::Delegate,
        };

        let expected_entry_point_type = match call.call.entry_point_type {
            EntryPointType::External => trace::EntryPointType::External,
            EntryPointType::L1Handler => trace::EntryPointType::L1Handler,
            EntryPointType::Constructor => trace::EntryPointType::Constructor,
        };

        let expected_storage_read_values = call.storage_read_values.clone();
        let expected_storage_keys: HashSet<Felt> =
            call.accessed_storage_keys.iter().map(|v| *v.key()).collect();
        let expected_inner_calls: Vec<_> =
            call.inner_calls.clone().into_iter().map(to_call_info).collect();

        let expected_gas_consumed = call.execution.gas_consumed as u128;
        let expected_failed = call.execution.failed;

        // convert to call info
        let call = to_call_info(call.clone());

        // assert actual values
        assert_eq!(call.contract_address, expected_contract_address);
        assert_eq!(call.caller_address, expected_caller_address);
        assert_eq!(call.code_address, expected_code_address);
        assert_eq!(call.class_hash, expected_class_hash);
        assert_eq!(call.entry_point_selector, expected_entry_point_selector);
        assert_eq!(call.calldata, expected_calldata);
        assert_eq!(call.retdata, expected_retdata);
        assert_eq!(call.events, expected_events);
        assert_eq!(call.l2_to_l1_messages, expected_l2_to_l1_msg);
        assert_eq!(call.call_type, expected_call_type);
        assert_eq!(call.entry_point_type, expected_entry_point_type);
        assert_eq!(call.storage_read_values, expected_storage_read_values);
        assert_eq!(call.accessed_storage_keys, expected_storage_keys);
        assert_eq!(call.inner_calls, expected_inner_calls);
        assert_eq!(call.gas_consumed, expected_gas_consumed);
        assert_eq!(call.failed, expected_failed);
    }
}
