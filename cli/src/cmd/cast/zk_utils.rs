/// This module handles transactions related to ZkSync. It defines the
/// CLI arguments for the `cast zk-send` command and provides functionality for sending
/// transactions and withdrawing from Layer 2 to Layer 1.
///
/// The module contains the following components:
/// - `ZkSendTxArgs`: Struct representing the command line arguments for the `cast zk-send` subcommand.
///     It includes parameters such as the destination address, function signature, function arguments,
///     withdraw flag, token to bridge, amount to bridge, transaction options, and Ethereum options.
/// - `ZkSendTxArgs` implementation: Defines methods to run the subcommand and print the transaction receipt.
/// - Helper functions:
///     - `print_receipt`: Prints the receipt of the transaction, including transaction hash, gas used,
///         effective gas price, block number, and deployed contract address.
///     - `get_to_address`: Retrieves the recipient address of the transaction.
///     - `parse_decimal_u256`: Parses a decimal string into a U256 number.
///     - `decode_hex`: Decodes a hexadecimal string into a byte vector.

pub mod zk_utils {
    use eyre::{eyre, Result};
    use foundry_config::{Chain, Config};
    use std::num::ParseIntError;
    use url::Url;
    use zksync::types::{H256, U256};
    use zksync_eth_signer::PrivateKeySigner;
    use zksync_types::PackedEthSignature;
    /// Gets the RPC URL for Ethereum.
    ///
    /// If the `eth.rpc_url` is `None`, an error is returned.
    ///
    /// # Returns
    ///
    /// A `Result` which is:
    /// - Ok: Contains the RPC URL as a String.
    /// - Err: Contains an error message indicating that the RPC URL was not provided.
    pub fn get_rpc_url(rpc_url: &Option<String>) -> eyre::Result<String> {
        match rpc_url {
            Some(url) => {
                let rpc_url = get_url_with_port(url)
                    .ok_or_else(|| eyre::Report::msg("Invalid RPC_URL"))?;
                Ok(rpc_url)
            },
            None => Err(eyre::Report::msg("RPC URL was not provided. Try using --rpc-url flag or environment variable 'ETH_RPC_URL= '")),
        }
    }

    /// Parses a URL string and attaches a default port if one is not specified.
    ///
    /// This function takes a URL string as input and attempts to parse it.
    /// If the URL string is not a valid URL, the function returns `None`.
    /// If the URL is valid and has a specified port, the function returns the URL as is.
    /// If the URL is valid but does not have a specified port, the function attaches a default port.
    /// The default port is 443 if the URL uses the HTTPS scheme, and 80 otherwise.
    ///
    /// # Parameters
    ///
    /// - `url_str`: The URL string to parse.
    ///
    /// # Returns
    ///
    /// An `Option` which contains a String with the parsed URL if successful, or `None` if the input was not a valid URL.
    pub fn get_url_with_port(url_str: &str) -> Option<String> {
        let url = Url::parse(url_str).ok()?;
        let default_port = url.scheme() == "https" && url.port().is_none();
        let port = url.port().unwrap_or_else(|| if default_port { 443 } else { 80 });
        Some(format!("{}://{}:{}{}", url.scheme(), url.host_str()?, port, url.path()))
    }

    /// Gets the private key from the Ethereum options.
    ///
    /// If the `eth.wallet.private_key` is `None`, an error is returned.
    ///
    /// # Returns
    ///
    /// A `Result` which is:
    /// - Ok: Contains the private key as `H256`.
    /// - Err: Contains an error message indicating that the private key was not provided.
    pub fn get_private_key(private_key: &Option<String>) -> Result<H256> {
        match private_key {
            Some(pkey) => {
                let val = decode_hex(&pkey)
                    .map_err(|e| eyre::Report::msg(format!("Error parsing private key: {}", e)))?;
                Ok(H256::from_slice(&val))
            }
            None => {
                Err(eyre::Report::msg("Private key was not provided. Try using --private-key flag"))
            }
        }
    }

    /// Gets the chain from the Ethereum options.
    ///
    /// If the `eth.chain` is `None`, an error is returned.
    ///
    /// # Returns
    ///
    /// A `Result` which is:
    /// - Ok: Contains the chain as `Chain`.
    /// - Err: Contains an error message indicating that the chain was not provided.
    pub fn get_chain(chain: Option<Chain>) -> Result<Chain> {
        match chain {
            Some(chain) => Ok(chain),
            None => Err(eyre::Report::msg(
                "Chain was not provided. Use --chain flag (ex. --chain 270 ) \nor environment variable 'CHAIN= ' (ex.'CHAIN=270')",
            )),
        }
    }

    /// Decodes a hexadecimal string into a byte vector.
    ///
    /// This function takes a hexadecimal string as input and decodes it into a vector of bytes.
    /// Each pair of hexadecimal characters in the input string represents one byte in the output vector.
    ///
    /// # Arguments
    ///
    /// * `s` - A string representing a hexadecimal value.
    ///
    /// # Returns
    ///
    /// A `Result` containing the decoded byte vector if successful, or a `ParseIntError` if the decoding fails.
    ///
    /// # Examples
    ///
    /// ```
    /// let hex_string = "48656c6c6f2c20576f726c6421";
    /// let bytes = decode_hex(hex_string).expect("Error decoding hex");
    /// assert_eq!(bytes, vec![72, 101, 108, 108, 111, 44, 32, 87, 111, 114, 108, 100, 33]);
    /// ```
    pub fn decode_hex(s: &str) -> std::result::Result<Vec<u8>, ParseIntError> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16)).collect()
    }
}
