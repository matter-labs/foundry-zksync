# RPC Endpoint - Foundry-ZKsync

This document details the CI changes made to both the **Rust endpoint selection logic and GitHub workflows** made in Foundry-ZKsync compared to upstream Foundry. Since upstream merges can affect this process, maintaining this documentation helps prevent time-consuming debugging of known issues in the CI. 

## Context
Foundry-zksync combines ZKsync tests with the upstream Foundry testing flow in CI. The system rotates between public RPC endpoints from different providers. Because these open endpoints handle regular CI runs and we depend on them, we hit rate-limiting errors. To solve this, **we are currently using a dedicated RPC Alchemy endpoint, provided by MatterLabs through GitHub secrets**. 



## Important files

```rust
        // Try Alchemy API Key from environment first for non-Mainnet chains
        if let Ok(alchemy_key) = std::env::var("ALCHEMY_API_KEY") {
            let subdomain_prefix = match chain {
                Optimism => Some("opt-mainnet"),
                Arbitrum => Some("arb-mainnet"),
                Polygon => Some("polygon-mainnet"),
                Sepolia => Some("eth-sepolia"),
                _ => None, // Only use Alchemy for configured chains
            };
            if let Some(subdomain_prefix) = subdomain_prefix {
                eprintln!("--- Using ALCHEMY_API_KEY env var for chain: {chain} ---");
                // Note: Key is leaked to get 'static str, matching previous pattern
                let key_ref: &'static str = Box::leak(alchemy_key.into_boxed_str());
                let host = format!("{subdomain_prefix}.g.alchemy.com");
                // Return the fully constructed Alchemy URL directly
                let url = if is_ws {
                    format!("wss://{host}/v2/{key_ref}")
                } else {
                    format!("https://{host}/v2/{key_ref}")
                };
                eprintln!("--- next_url(is_ws={is_ws}, chain={chain:?}) = {url} ---");
                return url;
            } else {
                eprintln!(
                    "--- ALCHEMY_API_KEY found, but chain {chain} not configured. Falling back. ---"
                );
            }
        } else {
            eprintln!("--- ALCHEMY_API_KEY not found. Falling back. ---");
        }
```

[https://github.com/matter-labs/foundry-zksync/blob/9b8658b6be691d80abb18fc4f8075a1c31d0f706/crates/test-utils/src/rpc.rs#L188](https://github.com/matter-labs/foundry-zksync/blob/9b8658b6be691d80abb18fc4f8075a1c31d0f706/crates/test-utils/src/rpc.rs#L188)

The ALCHEMY_API_KEY is defined in the GitHub secrets of the repository and passed through the necessary workflows

```yaml
      # Note(zk): Using our own Alchemy API key to avoid rate limiting issues
      ALCHEMY_API_KEY: ${{ secrets.ALCHEMY_API_KEY }}
```

[https://github.com/matter-labs/foundry-zksync/blob/9b8658b6be691d80abb18fc4f8075a1c31d0f706/.github/workflows/nextest.yml#L52](https://github.com/matter-labs/foundry-zksync/blob/9b8658b6be691d80abb18fc4f8075a1c31d0f706/.github/workflows/nextest.yml#L52)

This is a common issue that requires attention, as the [rpc.rs](http://rpc.rs) file frequently changes and can cause unexpected CI failures. Note that the ALCHEMY_API_KEY is passed to the zk-cargo-tests job, making it accessible through this code example:

```solidity
		forkEra = vm.createFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK)
		string memory ethUrl = string.concat(
            "https://eth-mainnet.alchemyapi.io/v2/",
            vm.envOr("ALCHEMY_API_KEY", string("ANY_API_KEY"))
        );
    forkEth = vm.createFork(ethUrl, ETH_FORK_BLOCK);
```

[https://github.com/matter-labs/foundry-zksync/blob/9b8658b6be691d80abb18fc4f8075a1c31d0f706/testdata/zk/Fork.t.sol#L42](https://github.com/matter-labs/foundry-zksync/blob/9b8658b6be691d80abb18fc4f8075a1c31d0f706/testdata/zk/Fork.t.sol#L42)