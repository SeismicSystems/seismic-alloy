//! Aliases to drop into foundry so we don't have to rename all the types
use alloy_serde::WithOtherFields;
use seismic_alloy_network::wallet::SeismicWallet;

pub use seismic_alloy_consensus::{
    Decodable712, SeismicReceiptEnvelope as AnyReceiptEnvelope, SeismicTxEnvelope as TxEnvelope,
    TxSeismic, TypedDataRequest, SEISMIC_TX_TYPE_ID
};
pub use seismic_alloy_network::{
    fillers::SeismicGasFiller as GasFiller,
    foundry::{
        block::{
            SeismicFoundryRpcBlock as AnyRpcBlock, SeismicFoundrySimBlock as SimBlock,
            SeismicFoundrySimulatePayload as SimulatePayload,
        },
        builder::seismic_foundry_tx_builder as tx_builder,
        envelope::SeismicFoundryTxEnvelope as AnyTxEnvelope,
        tx_request::{
            SeismicFoundryRpcTransaction as AnyRpcTransaction,
            SeismicFoundryTransactionRequest as AnyTransactionRequest,
            SeismicTransaction as RpcTransaction,
        },
        typed_tx::SeismicFoundryTypedTransaction as AnyTypedTransaction,
        SeismicFoundry as AnyNetwork,
    },
};
pub use seismic_alloy_provider::{
    provider::{
        sfoundry_signed_provider, sfoundry_unsigned_provider, SeismicSignedProvider,
        SeismicUnsignedProvider,
    },
    test_utils, SeismicProviderExt,
};
pub use seismic_alloy_rpc_types::{
    SeismicCallRequest, SeismicRawTxRequest, SeismicTransactionReceipt as TransactionReceipt,
    SeismicTransactionRequest as TransactionRequest, SeismicTransactionRequest,
};

/// A transaction receipt with the SeismicReceiptEnvelope wrapped in a WithOtherFields
pub type AnyTransactionReceipt = WithOtherFields<TransactionReceipt>;

/// A wallet for the Seismic network, renamed as EthereumWallet for compatibility with Foundry
pub type EthereumWallet = SeismicWallet<AnyNetwork>;

// Revm
use alloy_evm::EvmEnv as AlloyEvmEnv;
use revm::context::{CfgEnv as RevmCfgEnv, TxEnv as RevmTxEnv};

pub use alloy_seismic_evm::SeismicEvm;
pub use seismic_revm::{
    instructions::instruction_provider::{
        SeismicInstructions, SeismicInstructions as EthInstructions,
    },
    precompiles::SeismicPrecompiles,
    SeismicChain, SeismicContext as EthEvmContext, SeismicContext, SeismicEvm as RevmEvm,
    SeismicHaltReason as OpHaltReason, SeismicSpecId as SpecId, SeismicSpecId, SeismicTransaction,
    SeismicTransaction as OpTransaction,
};

/// Seismic transaction environment, which wraps revm's TxEnv
pub type TxEnv = SeismicTransaction<RevmTxEnv>;
/// Seismic configuration environment, which wraps revm's CfgEnv
pub type CfgEnv = RevmCfgEnv<SeismicSpecId>;
/// Seismic EVM environment, which wraps alloy-evm's EvmEnv
pub type EvmEnv = AlloyEvmEnv<SeismicSpecId>;
