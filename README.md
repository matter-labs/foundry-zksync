# **Foundry with ZKsync Era - Alpha**

**[Install](https://foundry-book.zksync.io/getting-started/installation)**
| [Limitations](https://foundry-book.zksync.io/zksync-specifics/limitations/)
| [User Book](https://foundry-book.zksync.io/)
| [ZKsync Docs](https://docs.zksync.io/build/tooling/foundry/overview)

**Foundry ZKsync** is a specialized fork of [Foundry](https://github.com/foundry-rs/foundry), tailored for ZKsync. 

It extends Foundry's capabilities for Ethereum app development to support ZKsync, **allowing for the compilation, deployment, testing, and interaction with smart contracts on ZKsync.**
 
> ⚠️ **Alpha Stage:** The project its alpha stage, indicating ongoing development and potential for future improvements.
>
> 🐞 **Found an Issue?:** Please report it to help us improve by opening an issue or submitting a pull request.

## 📖 User Book 

For **detailed information, including installation instructions, usage examples, and advanced guides**, please refer to the **[Foundry ZKsync Book](https://foundry-book.zksync.io/).**

*If you are interested in contributing to the book, please refer to the **[Foundry ZKsync Book repository](https://github.com/matter-labs/foundry-zksync-book).***

## 🤝 Contributing

See our [contributing guidelines](./CONTRIBUTING.md).

## 🗣️ Acknowledgements

### Foundry

-   Foundry is a clean-room rewrite of the testing framework [DappTools](https://github.com/dapphub/dapptools). None of this would have been possible without the DappHub team's work over the years.
-   [Matthias Seitz](https://twitter.com/mattsse_): Created [ethers-solc] which is the backbone of our compilation pipeline, as well as countless contributions to ethers, in particular the `abigen` macros.
-   [Rohit Narurkar](https://twitter.com/rohitnarurkar): Created the Rust Solidity version manager [svm-rs](https://github.com/roynalnaruto/svm-rs) which we use to auto-detect and manage multiple Solidity versions.
-   [Brock Elmore](https://twitter.com/brockjelmore): For extending the VM's cheatcodes and implementing [structured call tracing](https://github.com/foundry-rs/foundry/pull/192), a critical feature for debugging smart contract calls.
-   All the other [contributors](https://github.com/foundry-rs/foundry/graphs/contributors) to the [ethers-rs](https://github.com/gakonst/ethers-rs) & [foundry](https://github.com/foundry-rs/foundry) repositories and chatrooms.

### Foundry ZKsync
- [Moonsong Labs](https://moonsonglabs.com/): Implemented [ZKsync crates](./crates/zksync/), and resolved a number of different challenges to enable ZKsync support. 
