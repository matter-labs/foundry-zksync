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

First, see if the answer to your question can be found in the [Foundry Docs][foundry-docs], or in the relevant crate.

If the answer is not there:

- Join the [support Telegram][tg-support-url] to get help, or
- Open a [discussion](https://github.com/foundry-rs/foundry/discussions/new) with your question, or
- Open an issue with [the bug](https://github.com/foundry-rs/foundry/issues/new)

If you want to contribute, or follow along with contributor discussion, you can use our [main telegram](https://t.me/foundry_rs) to chat with us about the development of Foundry!

## License

Licensed under either of [Apache License](./LICENSE-APACHE), Version
2.0 or [MIT License](./LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in these crates by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.

## Acknowledgements

- Foundry is a clean-room rewrite of the testing framework [DappTools][dapptools]. None of this would have been possible without the DappHub team's work over the years.
- [Matthias Seitz](https://twitter.com/mattsse_): Created [ethers-solc] (now [foundry-compilers]) which is the backbone of our compilation pipeline, as well as countless contributions to ethers, in particular the `abigen` macros.
- [Rohit Narurkar](https://twitter.com/rohitnarurkar): Created the Rust Solidity version manager [svm-rs](https://github.com/roynalnaruto/svm-rs) which we use to auto-detect and manage multiple Solidity versions.
- [Brock Elmore](https://twitter.com/brockjelmore): For extending the VM's cheatcodes and implementing [structured call tracing](https://github.com/foundry-rs/foundry/pull/192), a critical feature for debugging smart contract calls.
- Thank you to [Depot](https://depot.dev) for sponsoring us with their fast GitHub runners and sccache, which we use in CI to reduce build and test times significantly.
- All the other [contributors](https://github.com/foundry-rs/foundry/graphs/contributors) to the [ethers-rs](https://github.com/gakonst/ethers-rs), [alloy][alloy] & [foundry](https://github.com/foundry-rs/foundry) repositories and chatrooms.

### Foundry ZKsync
- [Moonsong Labs](https://moonsonglabs.com/): Implemented [ZKsync crates](./crates/zksync/), and resolved a number of different challenges to enable ZKsync support. 
