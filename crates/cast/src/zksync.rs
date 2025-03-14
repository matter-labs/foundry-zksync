//! Contains zksync specific logic for foundry's `cast` functionality

use alloy_consensus::SignableTransaction;
use alloy_dyn_abi::FunctionExt;
use alloy_json_abi::Function;
use alloy_network::{AnyNetwork, NetworkWallet, TransactionBuilder};
use alloy_primitives::{hex, Address, Bytes, PrimitiveSignature, TxKind, U256};
use alloy_provider::Provider;
use alloy_rpc_types::{BlockId, TransactionRequest};
use alloy_serde::WithOtherFields;
use alloy_zksync::network::{
    transaction_request::TransactionRequest as ZkTransactionRequest,
    tx_envelope::TxEnvelope,
    unsigned_tx::{eip712::PaymasterParams, TypedTransaction},
    Zksync,
};
use clap::{command, Parser};
use eyre::{Context, Result};
use foundry_common::{
    fmt::{format_token, format_token_raw},
    shell,
};

use crate::Cast;

#[derive(Clone, Debug, Parser)]
#[command(next_help_heading = "Transaction options")]
pub struct ZkTransactionOpts {
    /// Paymaster address for the ZKSync transaction
    #[arg(long = "zk-paymaster-address", requires = "paymaster_input")]
    pub paymaster_address: Option<Address>,

    /// Paymaster input for the ZKSync transaction
    #[arg(long = "zk-paymaster-input", requires = "paymaster_address", value_parser = parse_hex_bytes)]
    pub paymaster_input: Option<Bytes>,

    /// Custom signature for the ZKSync transaction
    #[arg(long = "zk-custom-signature",  value_parser = parse_hex_bytes)]
    pub custom_signature: Option<Bytes>,

    /// Factory dependencies for the ZKSync transaction
    #[arg(long = "zk-factory-deps", value_parser = parse_hex_bytes, value_delimiter = ',')]
    pub factory_deps: Vec<Bytes>,

    /// Gas per pubdata for the ZKSync transaction
    #[arg(long = "zk-gas-per-pubdata")]
    pub gas_per_pubdata: Option<U256>,
}

fn parse_hex_bytes(s: &str) -> Result<Bytes, String> {
    hex::decode(s).map(Bytes::from).map_err(|e| format!("Invalid hex string: {e}"))
}

impl ZkTransactionOpts {
    pub fn has_zksync_args(&self) -> bool {
        self.paymaster_address.is_some() ||
            self.custom_signature.is_some() ||
            !self.factory_deps.is_empty() ||
            self.gas_per_pubdata.is_some()
    }

    /// Builds a base ZkSync transaction request from the common parameters
    pub fn build_base_tx(
        &self,
        evm_tx: WithOtherFields<TransactionRequest>,
        zk_code: Option<String>,
    ) -> Result<ZkTransactionRequest> {
        let is_create = evm_tx.to == Some(TxKind::Create);
        let mut tx: ZkTransactionRequest = evm_tx.inner.into();

        if let Some(gas_per_pubdata) = self.gas_per_pubdata {
            tx.set_gas_per_pubdata(gas_per_pubdata);
        }

        if let (Some(paymaster), Some(paymaster_input)) =
            (self.paymaster_address, self.paymaster_input.clone())
        {
            tx.set_paymaster_params(PaymasterParams { paymaster, paymaster_input });
        }

        if let Some(custom_signature) = self.custom_signature.clone() {
            tx.set_custom_signature(custom_signature);
        }

        if is_create {
            let input_data = tx.input().cloned().unwrap_or_default().to_vec();
            let zk_code = zk_code
                .ok_or_else(|| eyre::eyre!("ZkSync code is required for contract creation"))?;
            let zk_code_bytes = hex::decode(zk_code)?;
            let constructor_args = &input_data[zk_code_bytes.len()..];

            tx = tx.with_create_params(
                zk_code_bytes,
                constructor_args.to_vec(),
                self.factory_deps.iter().map(|b| b.to_vec()).collect(),
            )?;
        } else {
            tx.set_factory_deps(self.factory_deps.clone());
        }

        tx.prep_for_submission();
        Ok(tx)
    }
}

pub struct ZkCast<P, Z> {
    provider: Z,
    inner: Cast<P>,
}

impl<P, Z> AsRef<Cast<P>> for ZkCast<P, Z>
where
    P: Provider<AnyNetwork>,
    Z: Provider<Zksync>,
{
    fn as_ref(&self) -> &Cast<P> {
        &self.inner
    }
}

impl<P, Z> ZkCast<P, Z>
where
    P: Provider<AnyNetwork>,
    Z: Provider<Zksync>,
{
    /// Creates a new ZkCast instance from the provided client and Cast instance
    ///
    /// # Example
    ///
    /// ```
    /// use alloy_provider::{network::AnyNetwork, ProviderBuilder, RootProvider};
    /// use cast::Cast;
    ///
    /// # async fn foo() -> eyre::Result<()> {
    /// let provider =
    ///     ProviderBuilder::<_, _, AnyNetwork>::default().on_builtin("http://localhost:8545").await?;
    /// let cast = Cast::new(provider);
    /// let zk_provider =
    ///     ProviderBuilder::<_, _, Zksync>::default().on_builtin("http://localhost:8011").await?;
    /// let zk_cast = ZkCast::new(provider, cast);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(provider: Z, cast: Cast<P>) -> Self {
        Self { provider, inner: cast }
    }

    pub async fn call_zk(
        &self,
        req: &ZkTransactionRequest,
        func: Option<&Function>,
        block: Option<BlockId>,
    ) -> Result<String> {
        let res = self.provider.call(req.clone()).block(block.unwrap_or_default()).await?;

        let mut decoded = vec![];

        if let Some(func) = func {
            // decode args into tokens
            decoded = match func.abi_decode_output(res.as_ref(), false) {
                Ok(decoded) => decoded,
                Err(err) => {
                    // ensure the address is a contract
                    if res.is_empty() {
                        // check that the recipient is a contract that can be called
                        if let Some(TxKind::Call(addr)) = req.kind() {
                            if let Ok(code) = self
                                .provider
                                .get_code_at(addr)
                                .block_id(block.unwrap_or_default())
                                .await
                            {
                                if code.is_empty() {
                                    eyre::bail!("contract {addr:?} does not have any code")
                                }
                            }
                        } else if Some(TxKind::Create) == req.kind() {
                            eyre::bail!("tx req is a contract deployment");
                        } else {
                            eyre::bail!("recipient is None");
                        }
                    }
                    return Err(err).wrap_err(
                        "could not decode output; did you specify the wrong function return data type?"
                    );
                }
            };
        }

        // handle case when return type is not specified
        Ok(if decoded.is_empty() {
            res.to_string()
        } else if shell::is_json() {
            let tokens = decoded.iter().map(format_token_raw).collect::<Vec<_>>();
            serde_json::to_string_pretty(&tokens).unwrap()
        } else {
            // set compatible user-friendly return type conversions
            decoded.iter().map(format_token).collect::<Vec<_>>().join("\n")
        })
    }
}

/// Fills transaction with an empty signature. Used when custom signature is present
/// as a signed transaction is expected by alloy types as well as the Zksync node
/// which rlp decodes the signature but ignores it afterwards
#[derive(Debug, Clone)]
pub struct NoopWallet {
    pub address: Address,
}

impl NetworkWallet<Zksync> for NoopWallet {
    fn default_signer_address(&self) -> Address {
        self.address
    }

    fn has_signer_for(&self, address: &Address) -> bool {
        self.address == *address
    }

    fn signer_addresses(&self) -> impl Iterator<Item = Address> {
        [self.address].into_iter()
    }

    #[doc(alias = "sign_tx_from")]
    async fn sign_transaction_from(
        &self,
        _sender: Address,
        tx: TypedTransaction,
    ) -> alloy_signer::Result<TxEnvelope> {
        match tx {
            TypedTransaction::Native(_) => {
                Err(alloy_signer::Error::other("NoopWallet should only be used for zksync eip712 transactions with custom signature"))
            }
            TypedTransaction::Eip712(t) => {
                if t.eip712_meta.as_ref().map(|m| m.custom_signature.as_ref()).is_none() {
                    Err(alloy_signer::Error::other("NoopWallet should only be used for zksync eip712 transactions with custom signature"))
                } else {
                    let sig = PrimitiveSignature::try_from([0_u8; 65].as_slice()).unwrap();
                    Ok(TxEnvelope::Eip712(t.into_signed(sig)))
                }
            }
        }
    }
}
