# Changelog

## [0.0.11](https://github.com/matter-labs/foundry-zksync/compare/foundry-zksync-v0.0.10...foundry-zksync-v0.0.11) (2025-03-10)


### Features

* add support for custom signature in cast fields ([#935](https://github.com/matter-labs/foundry-zksync/issues/935)) ([4b59d03](https://github.com/matter-labs/foundry-zksync/commit/4b59d03591fccb6ecd3a21a8605e83c52122d969))
* cast call --trace support ([#953](https://github.com/matter-labs/foundry-zksync/issues/953)) ([0ff41b2](https://github.com/matter-labs/foundry-zksync/commit/0ff41b2be31c9bdc886327649793104fce12fbb0))
* upstream 4974a08 ([#946](https://github.com/matter-labs/foundry-zksync/issues/946)) ([f3a9825](https://github.com/matter-labs/foundry-zksync/commit/f3a9825a9b07bee979c94bc26cc80ccc96942eb5))


### Bug Fixes

* **inspector:** missing field in default ([f3a9825](https://github.com/matter-labs/foundry-zksync/commit/f3a9825a9b07bee979c94bc26cc80ccc96942eb5))

## [0.0.10](https://github.com/matter-labs/foundry-zksync/compare/foundry-zksync-v0.0.9...foundry-zksync-v0.0.10) (2025-02-26)


### Features

* support state diff cheatcode ([#922](https://github.com/matter-labs/foundry-zksync/issues/922)) ([a7b05d3](https://github.com/matter-labs/foundry-zksync/commit/a7b05d38b8a3e14cbda85abdac58dbb8bc19868a))
* Upstream 67be473 ([#924](https://github.com/matter-labs/foundry-zksync/issues/924)) ([ca9d1aa](https://github.com/matter-labs/foundry-zksync/commit/ca9d1aa4fb54e87f0e1ccad04e2b4f629b4520cc))


### Bug Fixes

* Add back test_zk_cast_call and test_zk_cast_call_create ([#933](https://github.com/matter-labs/foundry-zksync/issues/933)) ([b8a577e](https://github.com/matter-labs/foundry-zksync/commit/b8a577ed4816df84df437a5d456f9e04fe9012f5))
* consistent nonce w/ tx batching ([#929](https://github.com/matter-labs/foundry-zksync/issues/929)) ([ae9cfd1](https://github.com/matter-labs/foundry-zksync/commit/ae9cfd10d906b5ab350258533219da1f4775c118))
* Nonce mismatch when broadcasting in setup function  ([#923](https://github.com/matter-labs/foundry-zksync/issues/923)) ([cdef273](https://github.com/matter-labs/foundry-zksync/commit/cdef273ef198820ac6fa390bc868c6c8aa5cbab2))

## [0.0.9](https://github.com/matter-labs/foundry-zksync/compare/foundry-zksync-v0.0.8...foundry-zksync-v0.0.9) (2025-02-20)


### Features

* add support for cast estimate ([#903](https://github.com/matter-labs/foundry-zksync/issues/903)) ([5eaf1a7](https://github.com/matter-labs/foundry-zksync/commit/5eaf1a794e98086e3ff1900e384e62b95448dd70))
* Add support for cast send --create  ([#913](https://github.com/matter-labs/foundry-zksync/issues/913)) ([9aaddd6](https://github.com/matter-labs/foundry-zksync/commit/9aaddd6507039cf031599b81ede78fde75b28f6d))
* cast mktx and mktx --create support ([#911](https://github.com/matter-labs/foundry-zksync/issues/911)) ([4f89e2f](https://github.com/matter-labs/foundry-zksync/commit/4f89e2fd316edb4a8ad013779679ea55ce7a4e03))
* **cast:zk:** `call` and `call --create` support ([#917](https://github.com/matter-labs/foundry-zksync/issues/917)) ([2eaac65](https://github.com/matter-labs/foundry-zksync/commit/2eaac658af0152576ce6515db650787199a4abb7))
* inspect command ([#906](https://github.com/matter-labs/foundry-zksync/issues/906)) ([f7059a1](https://github.com/matter-labs/foundry-zksync/commit/f7059a156c1efb2f7c8dd350521bb0cf69e8809c))


### Bug Fixes

* foundry toml invalid setting does not discard other settings ([#912](https://github.com/matter-labs/foundry-zksync/issues/912)) ([27eda8b](https://github.com/matter-labs/foundry-zksync/commit/27eda8b39ee5fef6997c6993ccea771b0439b6db))
* Use new alchemy key to avoid rate limiting issues ([#919](https://github.com/matter-labs/foundry-zksync/issues/919)) ([89fa128](https://github.com/matter-labs/foundry-zksync/commit/89fa12899e7407576f29297b80b0fa161e6532c5))

## [0.0.8](https://github.com/matter-labs/foundry-zksync/compare/foundry-zksync-v0.0.7...foundry-zksync-v0.0.8) (2025-02-05)


### Features

* Improve display format ([#898](https://github.com/matter-labs/foundry-zksync/issues/898)) ([6af9b1a](https://github.com/matter-labs/foundry-zksync/commit/6af9b1af6c3da8396c951d3164887bd2af583faf))
* Upstream 9f11e6df ([#899](https://github.com/matter-labs/foundry-zksync/issues/899)) ([e98c3fe](https://github.com/matter-labs/foundry-zksync/commit/e98c3fe12c02142af92a5a755013983842f6fac5))
* Upstream 9f11e6df commits ([#900](https://github.com/matter-labs/foundry-zksync/issues/900)) ([13051f0](https://github.com/matter-labs/foundry-zksync/commit/13051f0ea45b2c7de3a0a2e56a0a4c08f6aaba5b))


### Bug Fixes

* use existing deployment nonce during storage migration ([#895](https://github.com/matter-labs/foundry-zksync/issues/895)) ([857d5d2](https://github.com/matter-labs/foundry-zksync/commit/857d5d252110dcbea3fe7731d3f38103c93a2dd7))

## [0.0.7](https://github.com/matter-labs/foundry-zksync/compare/foundry-zksync-v0.0.6...foundry-zksync-v0.0.7) (2025-01-31)


### Bug Fixes

* fix installation script for v0.0.6 ([#887](https://github.com/matter-labs/foundry-zksync/issues/887)) ([296c8f3](https://github.com/matter-labs/foundry-zksync/commit/296c8f3b14d7fe28ef6ed64592568cfed005e422))
* foundry man artifact name ([#886](https://github.com/matter-labs/foundry-zksync/issues/886)) ([62d5ff6](https://github.com/matter-labs/foundry-zksync/commit/62d5ff66cae2af74f4fad8524337a33394ae8437))

## [0.0.6](https://github.com/matter-labs/foundry-zksync/compare/foundry-zksync-v0.0.5...foundry-zksync-v0.0.6) (2025-01-31)


### Features

* implement compiler backwards compatibility policy ([#843](https://github.com/matter-labs/foundry-zksync/issues/843)) ([469b770](https://github.com/matter-labs/foundry-zksync/commit/469b7700404178060e6ee135ab967d723851bfa2))


### Bug Fixes

* trim tag name to obtain version ([#885](https://github.com/matter-labs/foundry-zksync/issues/885)) ([113501c](https://github.com/matter-labs/foundry-zksync/commit/113501c28a53e95393e20f1ab37df8848b472b95))

## [0.0.5](https://github.com/matter-labs/foundry-zksync/compare/foundry-zksync-v0.0.4...foundry-zksync-v0.0.5) (2025-01-29)


### Features

* add assemblycreate for warning suppression for zksolc 1.5.10 ([#840](https://github.com/matter-labs/foundry-zksync/issues/840)) ([62af6f9](https://github.com/matter-labs/foundry-zksync/commit/62af6f93260e470ae77501cfdcab27e94e9424de))
* Cache invalidation on zksolc version change ([#871](https://github.com/matter-labs/foundry-zksync/issues/871)) ([60a8f35](https://github.com/matter-labs/foundry-zksync/commit/60a8f35202d23064e589b3334be331fc42e31993))
* Upstream 5e72c69 ([#876](https://github.com/matter-labs/foundry-zksync/issues/876)) ([7b50143](https://github.com/matter-labs/foundry-zksync/commit/7b5014354a71a58e5a8e1326abe375ad0be988b4))
* **zk:** zksolc linking ([#800](https://github.com/matter-labs/foundry-zksync/issues/800)) ([b69695a](https://github.com/matter-labs/foundry-zksync/commit/b69695a020ba4d2850e069e7a0d53a03c5d92ac2))


### Bug Fixes

* add proper filter sets for ci test runs ([#875](https://github.com/matter-labs/foundry-zksync/issues/875)) ([886ff8b](https://github.com/matter-labs/foundry-zksync/commit/886ff8b4e4e3b466665f0236f30fb32bc6af1c2b))
* release artifact names and tags ([#866](https://github.com/matter-labs/foundry-zksync/issues/866)) ([b993907](https://github.com/matter-labs/foundry-zksync/commit/b993907c8c4873bdc720d7a942e0ae70466ed1be))
* Remove wrong estimation in create for zksync transactions ([#864](https://github.com/matter-labs/foundry-zksync/issues/864)) ([cf0a88d](https://github.com/matter-labs/foundry-zksync/commit/cf0a88d18218471dcf13d62afa4f8fe5335d9740))
* set log level for nonce revert to trace ([#873](https://github.com/matter-labs/foundry-zksync/issues/873)) ([a9289fb](https://github.com/matter-labs/foundry-zksync/commit/a9289fbb04b528a30d42d3daac5e62f250b04dc7))

## [0.0.4](https://github.com/matter-labs/foundry-zksync/compare/foundry-zksync-v0.0.3...foundry-zksync-v0.0.4) (2025-01-23)


### Bug Fixes

* platform name in release ([#862](https://github.com/matter-labs/foundry-zksync/issues/862)) ([4e12292](https://github.com/matter-labs/foundry-zksync/commit/4e12292801b5b412fc59fc9ba20137f8cf03f8c7))

## [0.0.3](https://github.com/matter-labs/foundry-zksync/compare/foundry-zksync-v0.0.2...foundry-zksync-v0.0.3) (2025-01-23)


### Features

* `--eof` flag and config key ([#9002](https://github.com/matter-labs/foundry-zksync/issues/9002)) ([ecf37f2](https://github.com/matter-labs/foundry-zksync/commit/ecf37f2f22d8e0700ead0ebae3bd3a27761c1236))
* **`--gas-report`:** add option to include tests ([#9232](https://github.com/matter-labs/foundry-zksync/issues/9232)) ([c90ea4d](https://github.com/matter-labs/foundry-zksync/commit/c90ea4d67f6a2492caa5d218d6c077388e3ef932))
* **`--gas-report`:** add option to show gas for tests ([c90ea4d](https://github.com/matter-labs/foundry-zksync/commit/c90ea4d67f6a2492caa5d218d6c077388e3ef932))
* **`anvil`:** `--cache-path` ([#9343](https://github.com/matter-labs/foundry-zksync/issues/9343)) ([057c8ac](https://github.com/matter-labs/foundry-zksync/commit/057c8ac20d2c2580237ed24557df846b48ab35b2))
* **`anvil`:** `wallet_` namespace + inject P256BatchDelegation + executor ([#9110](https://github.com/matter-labs/foundry-zksync/issues/9110)) ([08021d9](https://github.com/matter-labs/foundry-zksync/commit/08021d911a88a257739a6c8e6c957dfd1e1d6ee2))
* **`anvil`:** support mining with same block.timestamp ([#9160](https://github.com/matter-labs/foundry-zksync/issues/9160)) ([4d7435e](https://github.com/matter-labs/foundry-zksync/commit/4d7435e64ba1d351d128be3b1a30e6d6b246696a))
* **`cast run`:** add `--etherscan-api-key`  to resolve contract names ([#9295](https://github.com/matter-labs/foundry-zksync/issues/9295)) ([8c01706](https://github.com/matter-labs/foundry-zksync/commit/8c01706c96e457bac6a4d60be9c27ccbceca6396))
* **`cast`:** `decode-error` with sig, local cache and openchain api ([#9428](https://github.com/matter-labs/foundry-zksync/issues/9428)) ([0d76df5](https://github.com/matter-labs/foundry-zksync/commit/0d76df57a28236908084f21c965b20e30ed9dfdd))
* **`cast`:** `decode-event` with local and openchain API ([#9431](https://github.com/matter-labs/foundry-zksync/issues/9431)) ([0f7268f](https://github.com/matter-labs/foundry-zksync/commit/0f7268f46d2db7502cd0a75c8cfba34f06f8fd6e))
* **`cast`:** add flag equivalents of parseUnits, formatUnits  ([#9165](https://github.com/matter-labs/foundry-zksync/issues/9165)) ([bcef905](https://github.com/matter-labs/foundry-zksync/commit/bcef90556bd6755cedce16d7cd37c0f7f444b067))
* **`cheatcodes`:** `getArtifactPathByCode` and `getArtifactPathByDeployedCode` ([#8938](https://github.com/matter-labs/foundry-zksync/issues/8938)) ([c59d97e](https://github.com/matter-labs/foundry-zksync/commit/c59d97e8c1994684062f69305ce7cfacd52fceff))
* **`cheatcodes`:** access broadcast artifacts ([#9107](https://github.com/matter-labs/foundry-zksync/issues/9107)) ([2bb446e](https://github.com/matter-labs/foundry-zksync/commit/2bb446e9387b61d6fed1c157a7330b07c610b52e))
* **`cheatcodes`:** add `delegatecall` to `prank`ing ([#8863](https://github.com/matter-labs/foundry-zksync/issues/8863)) ([c526cab](https://github.com/matter-labs/foundry-zksync/commit/c526cab8364fdf410fb8b04d256ca83d4dc632bf))
* **`cheatcodes`:** count assertion for `expectRevert` ([#9484](https://github.com/matter-labs/foundry-zksync/issues/9484)) ([63484d0](https://github.com/matter-labs/foundry-zksync/commit/63484d0a65c56e3378cc3f282ed962d5d499a490))
* **`cheatcodes`:** mockCall with bytes4 data ([#9267](https://github.com/matter-labs/foundry-zksync/issues/9267)) ([adaad3d](https://github.com/matter-labs/foundry-zksync/commit/adaad3da964b18abaf425c7ce263ad0896a48cb5))
* **`cheatcodes`:** vm.getScriptWallets() ([#9052](https://github.com/matter-labs/foundry-zksync/issues/9052)) ([373ad46](https://github.com/matter-labs/foundry-zksync/commit/373ad46de9034f3b9e30b95084c9d1bd076d66a7))
* **`cheatcodes`:** vm.rememberKeys ([#9087](https://github.com/matter-labs/foundry-zksync/issues/9087)) ([9415dde](https://github.com/matter-labs/foundry-zksync/commit/9415dde6e6b4ce14bb773eab7a8ebe0ed8e0c52c))
* **`common::shell`:** add global verbosity level (`-vvv`) flag replacing `--verbose` ([#9273](https://github.com/matter-labs/foundry-zksync/issues/9273)) ([22cf683](https://github.com/matter-labs/foundry-zksync/commit/22cf683acf04180a96f4a4435fa34da34a502874))
* **`config`:** set default evm version to cancun ([#9131](https://github.com/matter-labs/foundry-zksync/issues/9131)) ([60dd1d7](https://github.com/matter-labs/foundry-zksync/commit/60dd1d7fe9879008a52da40eb74d5b6706d00b78))
* `DualCompiledContracts::find_bytecode` ([beb1108](https://github.com/matter-labs/foundry-zksync/commit/beb110865708abfc07427a46ced094bba3f22cd1))
* **`forge build -vvvvv`:** If verbosity level is 5 or higher show files to compile ([#9325](https://github.com/matter-labs/foundry-zksync/issues/9325)) ([7e323c2](https://github.com/matter-labs/foundry-zksync/commit/7e323c23463193f70c025f0df57b559a79db9676))
* **`forge build`:** `--watch` flag now watches `foundry.toml` config changes ([52b3da2](https://github.com/matter-labs/foundry-zksync/commit/52b3da2597e93bfda85fc650948945855e8e771e))
* **`forge build`:** `--watch` flag now watches `foundry.toml` config… ([#9148](https://github.com/matter-labs/foundry-zksync/issues/9148)) ([52b3da2](https://github.com/matter-labs/foundry-zksync/commit/52b3da2597e93bfda85fc650948945855e8e771e))
* **`forge build`:** add `--sizes` and `--names` JSON compatibility ([#9321](https://github.com/matter-labs/foundry-zksync/issues/9321)) ([a79dfae](https://github.com/matter-labs/foundry-zksync/commit/a79dfaed6fc6f88cda5f314a25d1b484d9d8c051))
* **`forge build`:** add initcode size check ([#9116](https://github.com/matter-labs/foundry-zksync/issues/9116)) ([8bdcbfa](https://github.com/matter-labs/foundry-zksync/commit/8bdcbfa4d65408b75c4038bd5ee67ce7f6dbd3bb))
* **`forge doc`:** include [@custom](https://github.com/custom) natspec ([#9075](https://github.com/matter-labs/foundry-zksync/issues/9075)) ([92702e9](https://github.com/matter-labs/foundry-zksync/commit/92702e9c0db4e76ddd7917fae4f74427a7e728f2))
* **`forge install`:** add `[@tag](https://github.com/tag)=` `[@branch](https://github.com/branch)=` `[@rev](https://github.com/rev)=` specific refs ([#9214](https://github.com/matter-labs/foundry-zksync/issues/9214)) ([a428ba6](https://github.com/matter-labs/foundry-zksync/commit/a428ba6ad8856611339a6319290aade3347d25d9))
* **`traces`:** show state changes in `cast run` and `forge test` on `-vvvvv` ([#9013](https://github.com/matter-labs/foundry-zksync/issues/9013)) ([c63aba8](https://github.com/matter-labs/foundry-zksync/commit/c63aba816b76f9bad103b1275cc662a063919403))
* add `--broadcast` flag to forge create, default to dry run mode ([#9420](https://github.com/matter-labs/foundry-zksync/issues/9420)) ([2c3114c](https://github.com/matter-labs/foundry-zksync/commit/2c3114c4d9cbe66a897e634b11b8771a56f91bec))
* add `foundry_common::shell` to unify log behavior ([#9109](https://github.com/matter-labs/foundry-zksync/issues/9109)) ([cd71da4](https://github.com/matter-labs/foundry-zksync/commit/cd71da404df324f8a3851f9673e4686d2cd762ef))
* add additional gas usage info from bootloader ([#590](https://github.com/matter-labs/foundry-zksync/issues/590)) ([8422d71](https://github.com/matter-labs/foundry-zksync/commit/8422d718f1b6e10f6571423648493cec7ccd4b79))
* Add binary attestations ([84cd0ae](https://github.com/matter-labs/foundry-zksync/commit/84cd0ae73c55243db2bead40af9df3d2538ed431))
* Add cargo nextest to zk tests ([#745](https://github.com/matter-labs/foundry-zksync/issues/745)) ([27360d4](https://github.com/matter-labs/foundry-zksync/commit/27360d4c8d12beddbb730dae07ad33a206b38f4b))
* add global -j, --threads ([#9367](https://github.com/matter-labs/foundry-zksync/issues/9367)) ([fef2098](https://github.com/matter-labs/foundry-zksync/commit/fef20981cbaa9c08e1ef1e3cd8bc57ccbcd1ac4e))
* add global `--json` flag ([#9244](https://github.com/matter-labs/foundry-zksync/issues/9244)) ([e2a6282](https://github.com/matter-labs/foundry-zksync/commit/e2a6282a52ebe62775ae4dda76d97898da4a1228))
* add JSON compatibility for `forge test --summary +/ --detailed` + apply consistent table styling ([#9485](https://github.com/matter-labs/foundry-zksync/issues/9485)) ([a4de7e8](https://github.com/matter-labs/foundry-zksync/commit/a4de7e812bca8962e7d30ab83890712adbf4a539))
* Add paymaster parameters to broadcasting flow ([#596](https://github.com/matter-labs/foundry-zksync/issues/596)) ([dbb13e7](https://github.com/matter-labs/foundry-zksync/commit/dbb13e7f1ffb9f74a2d1b87a7188fcb66dfb36e5))
* Add paymaster support to cast send ([#612](https://github.com/matter-labs/foundry-zksync/issues/612)) ([baee07e](https://github.com/matter-labs/foundry-zksync/commit/baee07edbcafac2ac011c5e918a9672d192b8fad))
* add retry workflow ([#638](https://github.com/matter-labs/foundry-zksync/issues/638)) ([db09e2c](https://github.com/matter-labs/foundry-zksync/commit/db09e2c42b03f6a617c86519f64717605a18b1ec))
* add strategy objects ([#781](https://github.com/matter-labs/foundry-zksync/issues/781)) ([5353a10](https://github.com/matter-labs/foundry-zksync/commit/5353a10345187933527fbad213d8c4f6500a775c))
* Add support for vm.getCode in Zk context ([#604](https://github.com/matter-labs/foundry-zksync/issues/604)) ([e498b34](https://github.com/matter-labs/foundry-zksync/commit/e498b3477b9f23b8a8d11aa591e96efce5c0370f))
* add the ability to use specific gas params in era vm environment and use them on script estimations ([#773](https://github.com/matter-labs/foundry-zksync/issues/773)) ([7b3c869](https://github.com/matter-labs/foundry-zksync/commit/7b3c86998c21ad85e3bd1a226c749619795adf88))
* add timeouts to fuzz testing ([#9394](https://github.com/matter-labs/foundry-zksync/issues/9394)) ([2e9f536](https://github.com/matter-labs/foundry-zksync/commit/2e9f53632a787323318e4575d7a0325ef3e7cc84))
* add vm error tracer ([#594](https://github.com/matter-labs/foundry-zksync/issues/594)) ([2aa8fa7](https://github.com/matter-labs/foundry-zksync/commit/2aa8fa7dba6759767ae6bf6c3e6c4ccd16a5f9ca))
* adds support for forge clean to remove zkout directory  ([#611](https://github.com/matter-labs/foundry-zksync/issues/611)) ([b513e39](https://github.com/matter-labs/foundry-zksync/commit/b513e390bf349481e4009c2906c2a13986d3fa4d))
* adds support for zksolc forge clean ([b513e39](https://github.com/matter-labs/foundry-zksync/commit/b513e390bf349481e4009c2906c2a13986d3fa4d))
* adds verification for zksync block explorer  ([#599](https://github.com/matter-labs/foundry-zksync/issues/599)) ([15bec2f](https://github.com/matter-labs/foundry-zksync/commit/15bec2f861b3b4c71e58f85e2b2c9dd722585aa8))
* allow any config to be defined inline ([#9430](https://github.com/matter-labs/foundry-zksync/issues/9430)) ([3e6d3b8](https://github.com/matter-labs/foundry-zksync/commit/3e6d3b8b6b96a02df1264294320a840ddc88345b))
* **anvil:** add `anvil_getIntervalMining` API ([#9290](https://github.com/matter-labs/foundry-zksync/issues/9290)) ([9df5939](https://github.com/matter-labs/foundry-zksync/commit/9df593939b995b08eee7dbab585ec368f65c8116))
* build static binaries ([#844](https://github.com/matter-labs/foundry-zksync/issues/844)) ([d00f5ae](https://github.com/matter-labs/foundry-zksync/commit/d00f5ae4af2ffeb2ec9a5a0f3dfe376e75327550))
* bump alpine to `3.20.3` ([#9094](https://github.com/matter-labs/foundry-zksync/issues/9094)) ([7a9ebf9](https://github.com/matter-labs/foundry-zksync/commit/7a9ebf9ccbce2957762ef1b3f4623efb76ef0306))
* bump MSRV to 1.83 ([#9473](https://github.com/matter-labs/foundry-zksync/issues/9473)) ([2f56133](https://github.com/matter-labs/foundry-zksync/commit/2f56133ce2e7d0d0d8b1488c2784dbd799d01e16))
* **cast:** add --int flag to from-rlp ([#9210](https://github.com/matter-labs/foundry-zksync/issues/9210)) ([00415bb](https://github.com/matter-labs/foundry-zksync/commit/00415bbb0653c429c1e21dcd0405be3005a36cc6))
* **cast:** add `--rpc-timeout` option ([#9044](https://github.com/matter-labs/foundry-zksync/issues/9044)) ([2559899](https://github.com/matter-labs/foundry-zksync/commit/25598999a2b33ac6ccfa35c347f3c98aba8e0061))
* **cast:** add `json` flag in `cast wallet new-mnemonic` ([#9139](https://github.com/matter-labs/foundry-zksync/issues/9139)) ([7c1c019](https://github.com/matter-labs/foundry-zksync/commit/7c1c019455686cdb277cfb3d47c15e22a59ae985))
* **cast:** add artifact method ([#9249](https://github.com/matter-labs/foundry-zksync/issues/9249)) ([f8d9234](https://github.com/matter-labs/foundry-zksync/commit/f8d92341baa030675db135d08a574f4caeb96177))
* **cast:** add contract creation bytecodes to traces ([#8941](https://github.com/matter-labs/foundry-zksync/issues/8941)) ([df2203c](https://github.com/matter-labs/foundry-zksync/commit/df2203cbb7c7945025c80a46b167b5a4fd118e94))
* **cast:** add decode-event sig data ([#9413](https://github.com/matter-labs/foundry-zksync/issues/9413)) ([31dd1f7](https://github.com/matter-labs/foundry-zksync/commit/31dd1f77fd9156d09836486d97963cec7f555343))
* **cast:** add string-decode to decode string ([#9237](https://github.com/matter-labs/foundry-zksync/issues/9237)) ([736a330](https://github.com/matter-labs/foundry-zksync/commit/736a3300234a0921b9d8adde6c0c4dd14053ec8a))
* **cast:** allow some more stdin inputs ([#9442](https://github.com/matter-labs/foundry-zksync/issues/9442)) ([d4e91c8](https://github.com/matter-labs/foundry-zksync/commit/d4e91c80266defb486c7b3626f44600f0cc1e0fc))
* **cast:** decode external lib sigs from cached selectors ([#9399](https://github.com/matter-labs/foundry-zksync/issues/9399)) ([16a013f](https://github.com/matter-labs/foundry-zksync/commit/16a013fafb519395dc1aca810dabc3fffb7d02a0))
* **cheatcode:** `startDebugTraceRecording` and `stopDebugTraceRecording` for ERC4337 testing ([#8571](https://github.com/matter-labs/foundry-zksync/issues/8571)) ([0c659f0](https://github.com/matter-labs/foundry-zksync/commit/0c659f07e1a3c1710ca5bc7c587f86620c2b1f8b))
* **cheatcodes:** add `vm.getStateDiff` to get state diffs as string ([#9435](https://github.com/matter-labs/foundry-zksync/issues/9435)) ([00efa0d](https://github.com/matter-labs/foundry-zksync/commit/00efa0d5965269149f374ba142fb1c3c7edd6c94))
* **cheatcodes:** Add `vm.mockCalls` to mock different return data for multiple calls ([#9024](https://github.com/matter-labs/foundry-zksync/issues/9024)) ([d7d9b40](https://github.com/matter-labs/foundry-zksync/commit/d7d9b407b20a5d2df1d06b07dafc1371a7e715b3))
* **cheatcodes:** add vm.cloneAccount() cheatcode ([#9048](https://github.com/matter-labs/foundry-zksync/issues/9048)) ([1ba5d6f](https://github.com/matter-labs/foundry-zksync/commit/1ba5d6fa58a80a5b24372f8a4894fc681bf0188a))
* **cheatcodes:** display warnings for deprecated cheatcodes ([#8883](https://github.com/matter-labs/foundry-zksync/issues/8883)) ([5725bcc](https://github.com/matter-labs/foundry-zksync/commit/5725bcc66899646c640f7feea3fa2bb3dfca753b))
* **cheatcodes:** implement new cheatcode to check if a string contains another string ([#9085](https://github.com/matter-labs/foundry-zksync/issues/9085)) ([bcacf39](https://github.com/matter-labs/foundry-zksync/commit/bcacf39e43812e50a124e3ba60d1becd9866534d))
* **cheatcodes:** random* cheatcodes to aid in symbolic testing ([#8882](https://github.com/matter-labs/foundry-zksync/issues/8882)) ([d15d71a](https://github.com/matter-labs/foundry-zksync/commit/d15d71ac0182e41091631225fcbb517926eda3fa))
* **cheatcodes:** skip test suite in setup ([#9532](https://github.com/matter-labs/foundry-zksync/issues/9532)) ([0eff1ef](https://github.com/matter-labs/foundry-zksync/commit/0eff1ef18fa1d21ec1280ed2b8b0f6e1549250ff))
* **chisel:** add eval command ([#9086](https://github.com/matter-labs/foundry-zksync/issues/9086)) ([15fdb2a](https://github.com/matter-labs/foundry-zksync/commit/15fdb2a19ee2a038f7e72523c6a0b0c3cdc6c3e4))
* compilation restrictions ([#8668](https://github.com/matter-labs/foundry-zksync/issues/8668)) ([547d8a5](https://github.com/matter-labs/foundry-zksync/commit/547d8a52ec7d286214511eb9c8ef5d5be601e81b))
* **compiler:zk:** zksolc 1.5.7 ([#688](https://github.com/matter-labs/foundry-zksync/issues/688)) ([953a180](https://github.com/matter-labs/foundry-zksync/commit/953a1800eaa2cffe5cb272f96ff641cc7a3c1ba1))
* **compiler:zk:** zksolc linking ([#711](https://github.com/matter-labs/foundry-zksync/issues/711)) ([f51b213](https://github.com/matter-labs/foundry-zksync/commit/f51b21333f5cabdca0e6326d4ae0624481786ab9))
* **coverage:** add --lcov-version ([e5dbb7a](https://github.com/matter-labs/foundry-zksync/commit/e5dbb7a320c2b871c4a4a1006ad3c15a08fcf17b))
* dedup error messages ([#9481](https://github.com/matter-labs/foundry-zksync/issues/9481)) ([8ac30d9](https://github.com/matter-labs/foundry-zksync/commit/8ac30d9c7ebeab1b50d98b56f6b5e623e7cdbf83))
* fix re-entrancy in strategies ([#801](https://github.com/matter-labs/foundry-zksync/issues/801)) ([6234614](https://github.com/matter-labs/foundry-zksync/commit/6234614bf052960b0c9b607e3c6bb1e07d764229))
* **fmt:** add `all_params` config - same as `all` but split single param too ([#9176](https://github.com/matter-labs/foundry-zksync/issues/9176)) ([b1e9365](https://github.com/matter-labs/foundry-zksync/commit/b1e93654348a0f31effa34790adae18865b14aa8))
* **forge build:** err if no source file in specified paths ([#9329](https://github.com/matter-labs/foundry-zksync/issues/9329)) ([9d7557f](https://github.com/matter-labs/foundry-zksync/commit/9d7557fcf0f758ea0e8ef5d2db853bd1e1d660dc))
* **forge, cast:** add `cast --with_local_artifacts`/`forge selectors cache` to trace with local artifacts ([#7359](https://github.com/matter-labs/foundry-zksync/issues/7359)) ([398ef4a](https://github.com/matter-labs/foundry-zksync/commit/398ef4a3d55d8dd769ce86cada5ec845e805188b))
* **forge:** add `compiler` subcommand ([#7909](https://github.com/matter-labs/foundry-zksync/issues/7909)) ([adb6aba](https://github.com/matter-labs/foundry-zksync/commit/adb6abae69c7a0d766db123f66686cc890c22dd0))
* **forge:** add max supported EVM version in compiler -vv ([#9129](https://github.com/matter-labs/foundry-zksync/issues/9129)) ([d5f6e34](https://github.com/matter-labs/foundry-zksync/commit/d5f6e34c39df6da5ad662036c869f3488e43393b))
* **forge:** allow `--verifier custom` option ([#9311](https://github.com/matter-labs/foundry-zksync/issues/9311)) ([36cbce7](https://github.com/matter-labs/foundry-zksync/commit/36cbce7c78b56dd68359084a5d8b03f84efed8fb))
* **forge:** allow passing value to --optimize ([641132f](https://github.com/matter-labs/foundry-zksync/commit/641132f5418bd7c268366c2da09e5300f3a8e272))
* **forge:** allow passing value to `--optimize` ([#9071](https://github.com/matter-labs/foundry-zksync/issues/9071)) ([641132f](https://github.com/matter-labs/foundry-zksync/commit/641132f5418bd7c268366c2da09e5300f3a8e272))
* **forge:** show additional details of contract to verify ([#9403](https://github.com/matter-labs/foundry-zksync/issues/9403)) ([eae5fb4](https://github.com/matter-labs/foundry-zksync/commit/eae5fb489d39b4de0a611778b9ce82233399e73e))
* foundry upstream support da77402 ([#601](https://github.com/matter-labs/foundry-zksync/issues/601)) ([dac2eea](https://github.com/matter-labs/foundry-zksync/commit/dac2eeab30eb631b7c1254c683ca66bbc151d239))
* gas snapshots over arbitrary sections  ([#8952](https://github.com/matter-labs/foundry-zksync/issues/8952)) ([08a6409](https://github.com/matter-labs/foundry-zksync/commit/08a6409ab742f33b398de0fb5bc6c24800677e8c))
* get rid of zksync_select_fork_vm method ([#813](https://github.com/matter-labs/foundry-zksync/issues/813)) ([e85875e](https://github.com/matter-labs/foundry-zksync/commit/e85875e10ca40afb2508d3489e488137b0b270b5))
* implement `parseTomlType` cheats ([#8911](https://github.com/matter-labs/foundry-zksync/issues/8911)) ([f2c14c1](https://github.com/matter-labs/foundry-zksync/commit/f2c14c176b6f69ede1c067bcfcc0fdf2d6beba5e))
* include anvil-zksync in foundryup install script ([#765](https://github.com/matter-labs/foundry-zksync/issues/765)) ([f6527ae](https://github.com/matter-labs/foundry-zksync/commit/f6527aedd7e573ae71e3135663e726df4fd3ec25))
* init forge project with zksync ([#586](https://github.com/matter-labs/foundry-zksync/issues/586)) ([8202db6](https://github.com/matter-labs/foundry-zksync/commit/8202db630bb5611ab70ffdd23afbe9323be781b9))
* Initial support for custom paymaster ([#591](https://github.com/matter-labs/foundry-zksync/issues/591)) ([22871de](https://github.com/matter-labs/foundry-zksync/commit/22871de47160b6578ecb2ef35c82a5d8a440f440))
* **invariant:** add basic metrics report ([#9158](https://github.com/matter-labs/foundry-zksync/issues/9158)) ([c2f1760](https://github.com/matter-labs/foundry-zksync/commit/c2f1760e22390ac66fc9adb9fdc9425a151cd0e3))
* make `--gas-report` JSON output compatible ([#9063](https://github.com/matter-labs/foundry-zksync/issues/9063)) ([0ec018d](https://github.com/matter-labs/foundry-zksync/commit/0ec018d34dc43600201d07386eaed41f97887028))
* make `--gas-report` w/ `--json` output one JSON blob and add `contract_path` to output ([#9216](https://github.com/matter-labs/foundry-zksync/issues/9216)) ([48930a6](https://github.com/matter-labs/foundry-zksync/commit/48930a68c583e8c56abd09e8b5af1cdb85367348))
* Paymaster support in Forge create ([#609](https://github.com/matter-labs/foundry-zksync/issues/609)) ([902f992](https://github.com/matter-labs/foundry-zksync/commit/902f992fc7ee31aa4f6f5b25ce79e878208683f0))
* **randomBytes:** adding support to generate different bytes via RngCore ([#8996](https://github.com/matter-labs/foundry-zksync/issues/8996)) ([d4649bf](https://github.com/matter-labs/foundry-zksync/commit/d4649bf5094f5c863a1795f7fbb19cc7efa52b4c))
* reintroduce backend.inspect ([#802](https://github.com/matter-labs/foundry-zksync/issues/802)) ([4e50046](https://github.com/matter-labs/foundry-zksync/commit/4e50046eebbe48d54229c6d3f11924dd21686c68))
* remove ethers ([#8826](https://github.com/matter-labs/foundry-zksync/issues/8826)) ([d739704](https://github.com/matter-labs/foundry-zksync/commit/d7397043e17e8d88a0c21cffa9d300377aed27c5))
* Removing `detect_missing_libraries` arg  ([#822](https://github.com/matter-labs/foundry-zksync/issues/822)) ([181e3ba](https://github.com/matter-labs/foundry-zksync/commit/181e3bacd06afd881ddb810090a16f522506fe0e))
* Removing bump-forge-std.yml ([#861](https://github.com/matter-labs/foundry-zksync/issues/861)) ([01021cd](https://github.com/matter-labs/foundry-zksync/commit/01021cdd56cf9f577434118e0cd08734f1525459))
* rename `ShellOtps` to `GlobalOpts` ([#9313](https://github.com/matter-labs/foundry-zksync/issues/9313)) ([622f922](https://github.com/matter-labs/foundry-zksync/commit/622f922739923ed243b1b5d701bb9e0898b3ffee))
* return actual gas charged to the transaction caller for zkVM transactions ([#592](https://github.com/matter-labs/foundry-zksync/issues/592)) ([d1b59e9](https://github.com/matter-labs/foundry-zksync/commit/d1b59e9ba588f072b08b1b6d52140f593b8e443b))
* rewrite inline config using figment ([#9414](https://github.com/matter-labs/foundry-zksync/issues/9414)) ([56d0dd8](https://github.com/matter-labs/foundry-zksync/commit/56d0dd8745248e9cd029472eb0a8697d12677246))
* rpc_headers in cast and config ([#9429](https://github.com/matter-labs/foundry-zksync/issues/9429)) ([af0fee2](https://github.com/matter-labs/foundry-zksync/commit/af0fee2031ed4273c1b697775650de1efb2a2d4e))
* **script:** support custom create2 deployer ([#9278](https://github.com/matter-labs/foundry-zksync/issues/9278)) ([7f41280](https://github.com/matter-labs/foundry-zksync/commit/7f41280ee071193557f73f16bae9aee9a5548ee8))
* switch to stable compiler ([#814](https://github.com/matter-labs/foundry-zksync/issues/814)) ([36db986](https://github.com/matter-labs/foundry-zksync/commit/36db986ab6747bead265bf60219291c47c9e1996))
* sync with upstream 59f354c ([#792](https://github.com/matter-labs/foundry-zksync/issues/792)) ([b24a80f](https://github.com/matter-labs/foundry-zksync/commit/b24a80fcf9cc24f250d7475c2378e4aea7c5e0c5))
* Update to soldeer 0.5.1 ([#9315](https://github.com/matter-labs/foundry-zksync/issues/9315)) ([78d263a](https://github.com/matter-labs/foundry-zksync/commit/78d263af61f37737c2f69fd94ec7fb8d2fc73987))
* Update to soldeer 0.5.2 ([#9373](https://github.com/matter-labs/foundry-zksync/issues/9373)) ([41b4359](https://github.com/matter-labs/foundry-zksync/commit/41b4359973235c37227a1d485cdb71dc56959b8b))
* update to Soldeer v0.4.0 ([#9014](https://github.com/matter-labs/foundry-zksync/issues/9014)) ([0b9bdf3](https://github.com/matter-labs/foundry-zksync/commit/0b9bdf35e14708cd88504bda55599eba196d21fc))
* update to Soldeer v0.4.1 ([#9092](https://github.com/matter-labs/foundry-zksync/issues/9092)) ([44b2d75](https://github.com/matter-labs/foundry-zksync/commit/44b2d754122c7ae98c03539e43b51efea6986c03)), closes [#212](https://github.com/matter-labs/foundry-zksync/issues/212)
* update to Soldeer v0.5.0 ([#9281](https://github.com/matter-labs/foundry-zksync/issues/9281)) ([c4a31a6](https://github.com/matter-labs/foundry-zksync/commit/c4a31a624874ab36284fca4e48d2197e43a62fbe))
* use multi-architecture images in Dockerfile to support apple si… ([#8964](https://github.com/matter-labs/foundry-zksync/issues/8964)) ([f7e9204](https://github.com/matter-labs/foundry-zksync/commit/f7e920488846629ba4977063d43b37a544d653a1))
* use multi-architecture images in Dockerfile to support apple silicon ([f7e9204](https://github.com/matter-labs/foundry-zksync/commit/f7e920488846629ba4977063d43b37a544d653a1))
* use new compilers api and add zkync solc test ([#607](https://github.com/matter-labs/foundry-zksync/issues/607)) ([cbd6940](https://github.com/matter-labs/foundry-zksync/commit/cbd69405e0be122b32e1376241d8ce8f735a34c6))
* use zk deployment sizes in gas report ([#595](https://github.com/matter-labs/foundry-zksync/issues/595)) ([46d9f92](https://github.com/matter-labs/foundry-zksync/commit/46d9f92bd6ee61a10bf259e10e49d8cb7e138c19))
* ZkUseFactoryDep cheatcode ([#671](https://github.com/matter-labs/foundry-zksync/issues/671)) ([9093c3c](https://github.com/matter-labs/foundry-zksync/commit/9093c3c3525d492aa77ecb070b90129eb0c1ef5f))


### Bug Fixes

* [#8759](https://github.com/matter-labs/foundry-zksync/issues/8759), default (low) gas limit set even when disabled, use custom gas_limit on forks ([#8933](https://github.com/matter-labs/foundry-zksync/issues/8933)) ([81fb0f6](https://github.com/matter-labs/foundry-zksync/commit/81fb0f60cc9f65c79eadbf50dd4b9e4907c522f7))
* **`--gas-report`:** add back signatures, even if empty, avoid nesting multiple selectors ([#9229](https://github.com/matter-labs/foundry-zksync/issues/9229)) ([748af79](https://github.com/matter-labs/foundry-zksync/commit/748af798223bd24e95394795109a0e683b42690c))
* **`--isolate`:** track state in journal ([#9018](https://github.com/matter-labs/foundry-zksync/issues/9018)) ([d3ce9f0](https://github.com/matter-labs/foundry-zksync/commit/d3ce9f08294bf3e78d0d3167f9b4a4669e262600))
* **`anvil`:** arb fork mining ([#9153](https://github.com/matter-labs/foundry-zksync/issues/9153)) ([1af44bf](https://github.com/matter-labs/foundry-zksync/commit/1af44bf750e6c3917dcdcaf8f853a44aacb061ad))
* **`anvil`:** eth_gasPrice returned `1000000000` with `--block-base-fee-per-gas 0`, adds new `--disable-min-priority-fee` to return `0` ([#9049](https://github.com/matter-labs/foundry-zksync/issues/9049)) ([e215f3f](https://github.com/matter-labs/foundry-zksync/commit/e215f3fdeada259a8886a7611151794d280ca298))
* **`anvil`:** handle OP deposit txs in `TypedTransaction` and `PoolTransaction` conversion ([#8942](https://github.com/matter-labs/foundry-zksync/issues/8942)) ([c9d7b48](https://github.com/matter-labs/foundry-zksync/commit/c9d7b48fb0cdddc33c61db82fc3a94dd7e602c9e))
* **`anvil`:** impl `maybe_as_full_db` for `ForkedDatabase` ([#9465](https://github.com/matter-labs/foundry-zksync/issues/9465)) ([9af381f](https://github.com/matter-labs/foundry-zksync/commit/9af381f91e7ad10d1bd34255a3af5fad34b9573b))
* **`anvil`:** set `storage.best_hash` while loading state ([#9021](https://github.com/matter-labs/foundry-zksync/issues/9021)) ([67018dc](https://github.com/matter-labs/foundry-zksync/commit/67018dcf3cc4ee80471a6d8a4d519c1d946b7fbb))
* **`anvil`:** set `storage.best_number` correctly ([#9215](https://github.com/matter-labs/foundry-zksync/issues/9215)) ([45d5997](https://github.com/matter-labs/foundry-zksync/commit/45d5997134e9de548a99a46367023c1ea4625073))
* **`anvil`:** tag newly created legacy transactions on shadow fork as `Some(0)` (`0x0`) rather than `None` ([#9195](https://github.com/matter-labs/foundry-zksync/issues/9195)) ([192a5a2](https://github.com/matter-labs/foundry-zksync/commit/192a5a24919de3eed36c92cc48cd29d55dc991b7))
* **`anvil`:** use header.number not best_number ([#9151](https://github.com/matter-labs/foundry-zksync/issues/9151)) ([6d9951f](https://github.com/matter-labs/foundry-zksync/commit/6d9951fce6ed482ec6717c104b9795d3cc3bb346))
* **`cast block`:** ensure to print all fields ([#9209](https://github.com/matter-labs/foundry-zksync/issues/9209)) ([5c69a9d](https://github.com/matter-labs/foundry-zksync/commit/5c69a9d9fd4e2ec07fc398ab5ef9d706c33890c2))
* **`cheatcodes`:** mark `vm.breakpoint` as `pure`  ([#9051](https://github.com/matter-labs/foundry-zksync/issues/9051)) ([47f1ecb](https://github.com/matter-labs/foundry-zksync/commit/47f1ecb9c6f7e251c5bf2452c1f327d5508481a9))
* **`ci`:** update cargo deny ([#9314](https://github.com/matter-labs/foundry-zksync/issues/9314)) ([4304926](https://github.com/matter-labs/foundry-zksync/commit/4304926fe0834af65a5cbc9b26c869e8c748d097))
* **`cli`:** handle id and named chain_id's correctly ([#9480](https://github.com/matter-labs/foundry-zksync/issues/9480)) ([3a1e76b](https://github.com/matter-labs/foundry-zksync/commit/3a1e76b504348e3fd90196e445fc04934f05680c))
* **`coverage`:** allow `ir-minimum` for versions &lt; 0.8.5 ([#9341](https://github.com/matter-labs/foundry-zksync/issues/9341)) ([dacf341](https://github.com/matter-labs/foundry-zksync/commit/dacf3410e84bab1d8bab34a3c53364ab4fca4063))
* **`deps`:** update `alloy-chains` to fix Celo explorer API URL ([#9242](https://github.com/matter-labs/foundry-zksync/issues/9242)) ([9511462](https://github.com/matter-labs/foundry-zksync/commit/95114622e832ca93a95004c5846c85e5ba81ba62))
* **`evm`:** detect blob tx and set evm version ([#9185](https://github.com/matter-labs/foundry-zksync/issues/9185)) ([c6d59b3](https://github.com/matter-labs/foundry-zksync/commit/c6d59b32fad4b78453354b92acfef5a95013b17f))
* **`evm`:** set blob_excess_gas_and_price ([#9186](https://github.com/matter-labs/foundry-zksync/issues/9186)) ([b74e467](https://github.com/matter-labs/foundry-zksync/commit/b74e467e1047d0ac854bbc35f603a83e94fc13b8))
* **`forge doc`:** display custom natspec tag ([#9257](https://github.com/matter-labs/foundry-zksync/issues/9257)) ([32f8e79](https://github.com/matter-labs/foundry-zksync/commit/32f8e798298443565c789883206bd024b46c4712))
* **`forge eip712`:** fix handling of subtypes ([#9035](https://github.com/matter-labs/foundry-zksync/issues/9035)) ([f089dff](https://github.com/matter-labs/foundry-zksync/commit/f089dff1c6c24d1ddf43c7cbefee46ea0197c88f))
* **`forge eip712`:** handle recursive types ([#9319](https://github.com/matter-labs/foundry-zksync/issues/9319)) ([a65a5b1](https://github.com/matter-labs/foundry-zksync/commit/a65a5b1445ba7ec9b10baf7ecb28f7a65bbb13ce))
* **`forge test`:** record only test fns in test failures ([#9286](https://github.com/matter-labs/foundry-zksync/issues/9286)) ([f3376a6](https://github.com/matter-labs/foundry-zksync/commit/f3376a6e45ffacd45125e639e5f50bec0c0900be))
* **`forge`:** avoid panic when empty fuzz selectors in invariants ([#9076](https://github.com/matter-labs/foundry-zksync/issues/9076)) ([d847e0f](https://github.com/matter-labs/foundry-zksync/commit/d847e0f09a95ef6ff8463521b98136e74dac37da))
* **`forge`:** run `dep.has_branch` in correct dir  ([#9453](https://github.com/matter-labs/foundry-zksync/issues/9453)) ([ade4b35](https://github.com/matter-labs/foundry-zksync/commit/ade4b35eedbab9ebe9511c7a70cd371a4b7ed2bb))
* **`forge`:** run git cmd in correct dir ([ade4b35](https://github.com/matter-labs/foundry-zksync/commit/ade4b35eedbab9ebe9511c7a70cd371a4b7ed2bb))
* **`invariant`:** replay should not fail for magic assume ([#8966](https://github.com/matter-labs/foundry-zksync/issues/8966)) ([20cb903](https://github.com/matter-labs/foundry-zksync/commit/20cb9038e203c2f11162e9e3b91db22f25a71c76))
* `vm.broadcastRawTransaction` ([#9378](https://github.com/matter-labs/foundry-zksync/issues/9378)) ([2bc7125](https://github.com/matter-labs/foundry-zksync/commit/2bc7125e913b211b2d6c59ecdc5f1f427440652b))
* 4844 fee fixes ([#8963](https://github.com/matter-labs/foundry-zksync/issues/8963)) ([25f24e6](https://github.com/matter-labs/foundry-zksync/commit/25f24e677a6a32a62512ad4f561995589ac2c7dc))
* add back `silent` option in Anvil's `NodeConfig` ([#9181](https://github.com/matter-labs/foundry-zksync/issues/9181)) ([3ff0cdd](https://github.com/matter-labs/foundry-zksync/commit/3ff0cddea7e19ff00c94c92f6173092e7938086c))
* Add check in ZK create to check if it's the test contract ([#585](https://github.com/matter-labs/foundry-zksync/issues/585)) ([082b6a3](https://github.com/matter-labs/foundry-zksync/commit/082b6a3610be972dd34aff9439257f4d85ddbf15))
* Add gas limit estimation using paymaster balance when in use ([#694](https://github.com/matter-labs/foundry-zksync/issues/694)) ([d9babce](https://github.com/matter-labs/foundry-zksync/commit/d9babce50528057735a52abb29f410e7c06e765f))
* Add missing injection of factory deps ([#753](https://github.com/matter-labs/foundry-zksync/issues/753)) ([2eb72ab](https://github.com/matter-labs/foundry-zksync/commit/2eb72ab569283c46f2cd4994da5c23d209455f85))
* Add transaction type to zk tx in cast send ([#767](https://github.com/matter-labs/foundry-zksync/issues/767)) ([1ca471f](https://github.com/matter-labs/foundry-zksync/commit/1ca471f4f625196466a948246d74351e09447dfb))
* add verbosity for verification compilation error ([#703](https://github.com/matter-labs/foundry-zksync/issues/703)) ([81a9bad](https://github.com/matter-labs/foundry-zksync/commit/81a9baddbb244fe61c447dd5545b8440f07b318c))
* Add zkout to gitignore template when in zksync forge init mode ([#772](https://github.com/matter-labs/foundry-zksync/issues/772)) ([c9def31](https://github.com/matter-labs/foundry-zksync/commit/c9def31f50b47ebebde89c7d87d1e490e5b92def))
* allow_hyphen_values for constructor args ([#9225](https://github.com/matter-labs/foundry-zksync/issues/9225)) ([4012ade](https://github.com/matter-labs/foundry-zksync/commit/4012adefd376bd618d1348398c1da07224d2dace))
* **anvil:** Apply state overrides in debug_traceCall ([#9172](https://github.com/matter-labs/foundry-zksync/issues/9172)) ([4c84dc7](https://github.com/matter-labs/foundry-zksync/commit/4c84dc7d9150d85794363402f959c3fe5ee28a55))
* **anvil:** correctly set hardfork-specific block fields ([#9202](https://github.com/matter-labs/foundry-zksync/issues/9202)) ([1229278](https://github.com/matter-labs/foundry-zksync/commit/12292787208c626ed6b2791eeed55ef7ab3578b0))
* **anvil:** on anvil_mine jump to next timestamp before mine new block ([#9241](https://github.com/matter-labs/foundry-zksync/issues/9241)) ([6b0c27e](https://github.com/matter-labs/foundry-zksync/commit/6b0c27ed4ccfdb5a4805e9f53d487cca51c5e116))
* **anvil:** set auto-unlock an alias of auto-impersonate ([#9256](https://github.com/matter-labs/foundry-zksync/issues/9256)) ([57bb12e](https://github.com/matter-labs/foundry-zksync/commit/57bb12e022fb9ea46a4a7ca8647eb016e8d43ca3))
* avoid deadlock in nested shell calls ([#9245](https://github.com/matter-labs/foundry-zksync/issues/9245)) ([ea11082](https://github.com/matter-labs/foundry-zksync/commit/ea11082555e15f899a8bb9102890f3c2f7713cb8))
* bail incomplete bytecode sequence disassemble ([#9390](https://github.com/matter-labs/foundry-zksync/issues/9390)) ([cca72ab](https://github.com/matter-labs/foundry-zksync/commit/cca72aba47a675380a3c87199c7ed0406e3281c2))
* better error handling when waiting for receipt ([#9253](https://github.com/matter-labs/foundry-zksync/issues/9253)) ([d402afd](https://github.com/matter-labs/foundry-zksync/commit/d402afd2db0e4546d33a7f94d3a226cce6ff2c76))
* **cast block:** ensure to print all fields ([5c69a9d](https://github.com/matter-labs/foundry-zksync/commit/5c69a9d9fd4e2ec07fc398ab5ef9d706c33890c2))
* **cast storage:** respect `--json` for layout ([#9332](https://github.com/matter-labs/foundry-zksync/issues/9332)) ([d275a49](https://github.com/matter-labs/foundry-zksync/commit/d275a4901f60a50c5a82fcf10fd5774ddb4598d8))
* **cast:** do not strip 0x / hex decode message before EIP-191 hashing ([#9130](https://github.com/matter-labs/foundry-zksync/issues/9130)) ([ca49147](https://github.com/matter-labs/foundry-zksync/commit/ca4914772d3162ece49cfa3d2c6c6b28e4d48118))
* **cheatcodes:** chain report source errors ([2044fae](https://github.com/matter-labs/foundry-zksync/commit/2044faec64f99a21f0e5f0094458a973612d0712))
* **cheatcodes:** clear orderings together with trace steps on debug trace stop ([#9529](https://github.com/matter-labs/foundry-zksync/issues/9529)) ([b090638](https://github.com/matter-labs/foundry-zksync/commit/b0906386497c03aef53f67b929ca6418aebe34ed))
* **cheatcodes:** convert fixed bytes to bytes in vm.rpc tuple result ([#9117](https://github.com/matter-labs/foundry-zksync/issues/9117)) ([3786b27](https://github.com/matter-labs/foundry-zksync/commit/3786b27150e9c444cbb060d6d991ebf867733e38))
* **cheatcodes:** empty ordering and step logs too ([b090638](https://github.com/matter-labs/foundry-zksync/commit/b0906386497c03aef53f67b929ca6418aebe34ed))
* **cheatcodes:** fix deploy create with broadcastRawTransaction ([a355af4](https://github.com/matter-labs/foundry-zksync/commit/a355af4750c4e12103e9684f99401b5b14cd23f9))
* **cheatcodes:** handle create2 deployer with broadcastRawTransaction ([#9020](https://github.com/matter-labs/foundry-zksync/issues/9020)) ([a355af4](https://github.com/matter-labs/foundry-zksync/commit/a355af4750c4e12103e9684f99401b5b14cd23f9))
* **cheatcodes:** improve fork cheatcodes messages ([#9141](https://github.com/matter-labs/foundry-zksync/issues/9141)) ([2044fae](https://github.com/matter-labs/foundry-zksync/commit/2044faec64f99a21f0e5f0094458a973612d0712))
* **cheatcodes:** use calldata in attachDelegation ([#9407](https://github.com/matter-labs/foundry-zksync/issues/9407)) ([672bdf6](https://github.com/matter-labs/foundry-zksync/commit/672bdf60f01630d849f0bf7ffdb447965a53e4e2))
* **chisel:** final statement & fetch err with complex type fixes ([#9081](https://github.com/matter-labs/foundry-zksync/issues/9081)) ([4065d38](https://github.com/matter-labs/foundry-zksync/commit/4065d38cec998608a3e3042a7c577f72fb586ed4))
* **chisel:** on edit fail command only if execution failed ([#9155](https://github.com/matter-labs/foundry-zksync/issues/9155)) ([9fe891a](https://github.com/matter-labs/foundry-zksync/commit/9fe891ab5babbdc2891c67d14d6c75ea1ca4b19c))
* **chisel:** uint/int full word print ([#9381](https://github.com/matter-labs/foundry-zksync/issues/9381)) ([cf66dea](https://github.com/matter-labs/foundry-zksync/commit/cf66dea727a6c7f41fa48fbe6dcabe474bfbfd79))
* **ci:** flexibly handle forge-std being installed with tag or untagged ([#9003](https://github.com/matter-labs/foundry-zksync/issues/9003)) ([452066e](https://github.com/matter-labs/foundry-zksync/commit/452066e9747a28682a4de069a05b10fe9f381167))
* **ci:** run tests in hosted runner ([a4f0ebf](https://github.com/matter-labs/foundry-zksync/commit/a4f0ebf8ea8f0725196b051b89dcf0ef7c49caed))
* **ci:** Update github-hosted runner label ([#846](https://github.com/matter-labs/foundry-zksync/issues/846)) ([5c46959](https://github.com/matter-labs/foundry-zksync/commit/5c4695922dde0cbce1cf06aefab2ea5bd8d4c9dc))
* **cli:** etherlink needs eth_estimateGas for gas calculation ([#9188](https://github.com/matter-labs/foundry-zksync/issues/9188)) ([ab8ebf6](https://github.com/matter-labs/foundry-zksync/commit/ab8ebf667d04eaeb0826adf17cc238c5a6719936))
* compute stored test address after setting the tx nonce on scripts ([b40ebe1](https://github.com/matter-labs/foundry-zksync/commit/b40ebe18341f8000fb840028049a757d953ff094))
* correct shell substitution in installer ([#9351](https://github.com/matter-labs/foundry-zksync/issues/9351)) ([d20c142](https://github.com/matter-labs/foundry-zksync/commit/d20c142d0655490122e79fb66aa119df3638bad6))
* **coverage:** also ignore empty fallbacks and receives ([#9459](https://github.com/matter-labs/foundry-zksync/issues/9459)) ([ee9d237](https://github.com/matter-labs/foundry-zksync/commit/ee9d23723efe7893c10547371d830b24bd2aab13))
* **coverage:** assert should not be branch ([#9467](https://github.com/matter-labs/foundry-zksync/issues/9467)) ([9ee6005](https://github.com/matter-labs/foundry-zksync/commit/9ee60053de47ce18ca76ff7f2da41ab026df17f9))
* **coverage:** better find of loc start byte position ([#8958](https://github.com/matter-labs/foundry-zksync/issues/8958)) ([8d5a66d](https://github.com/matter-labs/foundry-zksync/commit/8d5a66d90cfbf3e68b0188112898735cdd7562e9))
* **coverage:** clean ups, use normalized source code for locations ([#9438](https://github.com/matter-labs/foundry-zksync/issues/9438)) ([7a23a5c](https://github.com/matter-labs/foundry-zksync/commit/7a23a5cf851b991bfd2fde32d4f088319bbc1183))
* **coverage:** do not report empty constructors, enable reports for `receive` ([#9288](https://github.com/matter-labs/foundry-zksync/issues/9288)) ([91d3349](https://github.com/matter-labs/foundry-zksync/commit/91d33495a41530fc5ff78cb5ed26d6d17ade93e0))
* **coverage:** do not report empty constructors, enable reports for receive fn ([91d3349](https://github.com/matter-labs/foundry-zksync/commit/91d33495a41530fc5ff78cb5ed26d6d17ade93e0))
* **coverage:** special functions have no name ([#9441](https://github.com/matter-labs/foundry-zksync/issues/9441)) ([168b239](https://github.com/matter-labs/foundry-zksync/commit/168b239486c834d9d1fafdd98950e377c044b4db))
* **create:zk:** avoid initializing signer twice ([3653a8d](https://github.com/matter-labs/foundry-zksync/commit/3653a8dec08a5e9de1f8cda50e0959db8304f1c2))
* **create:zk:** find by any bytecode type ([909c95c](https://github.com/matter-labs/foundry-zksync/commit/909c95c34fd41ae4a8451574781f92d1ab1ab983))
* default TransactionRequest ([#787](https://github.com/matter-labs/foundry-zksync/issues/787)) ([68fcb7d](https://github.com/matter-labs/foundry-zksync/commit/68fcb7dc19eda1b57bf8b4c9229daefede17e7df))
* **deny:** instant and derivative are unmaintained ([#730](https://github.com/matter-labs/foundry-zksync/issues/730)) ([a088464](https://github.com/matter-labs/foundry-zksync/commit/a0884649b129743af09ba1b55579914f0f2b58d1))
* Display name of zksync verifier ([#758](https://github.com/matter-labs/foundry-zksync/issues/758)) ([77198c6](https://github.com/matter-labs/foundry-zksync/commit/77198c605dfb771f526dd74927750d27d5de33f4))
* do not handle expectCall in zkEVM ([#807](https://github.com/matter-labs/foundry-zksync/issues/807)) ([9be4421](https://github.com/matter-labs/foundry-zksync/commit/9be4421ba2de27aa1cf98867545c8c9ebbe78635))
* dont set state root ([#9134](https://github.com/matter-labs/foundry-zksync/issues/9134)) ([7cbd55e](https://github.com/matter-labs/foundry-zksync/commit/7cbd55e5b1b655f3855a816e16e954de83bb6b51))
* enable `revm/blst` ([#8965](https://github.com/matter-labs/foundry-zksync/issues/8965)) ([e485eeb](https://github.com/matter-labs/foundry-zksync/commit/e485eebec933d5e615fe968264e58ca4adfd951d))
* Encode correctly paymaster input ([#757](https://github.com/matter-labs/foundry-zksync/issues/757)) ([2b973e1](https://github.com/matter-labs/foundry-zksync/commit/2b973e16ba7b03f38c19faca8883f58a44d5529b))
* erroneous nonce updates ([#839](https://github.com/matter-labs/foundry-zksync/issues/839)) ([7630fbd](https://github.com/matter-labs/foundry-zksync/commit/7630fbd8bc618c58537ff1e891e0638d6004ee09))
* Exclude from empty code error case where there is no calldata and value ([#804](https://github.com/matter-labs/foundry-zksync/issues/804)) ([1cd2c56](https://github.com/matter-labs/foundry-zksync/commit/1cd2c56e4bffb415b0df64a3e8c2314ee01323d1))
* fix `test.yml` errors ([#774](https://github.com/matter-labs/foundry-zksync/issues/774)) ([ab01854](https://github.com/matter-labs/foundry-zksync/commit/ab01854fb857d6592290248774900362d2df8531))
* fix foundry-zksync-install CI check ([#735](https://github.com/matter-labs/foundry-zksync/issues/735)) ([0c43026](https://github.com/matter-labs/foundry-zksync/commit/0c4302670fd289be3d3be70973fdd1f7fe92503d))
* Fixed Forge version check ([#733](https://github.com/matter-labs/foundry-zksync/issues/733)) ([5ea23d2](https://github.com/matter-labs/foundry-zksync/commit/5ea23d24d1821442bfa8aa61688ee3c6ccc09611))
* flaky test_broadcast_raw_create2_deployer ([#9383](https://github.com/matter-labs/foundry-zksync/issues/9383)) ([37cc284](https://github.com/matter-labs/foundry-zksync/commit/37cc284f939a55bc1886e4bb7ba6ca99930fb4ee))
* **fmt:** do not panic when no named arg ([#9114](https://github.com/matter-labs/foundry-zksync/issues/9114)) ([440837d](https://github.com/matter-labs/foundry-zksync/commit/440837d3e71c4cd4c551352bbc8486110a1db44d))
* **fmt:** multiline single param only if func definition is multiline for `all_params` ([#9187](https://github.com/matter-labs/foundry-zksync/issues/9187)) ([216b60a](https://github.com/matter-labs/foundry-zksync/commit/216b60a9467a29c89da578ba4495afd1dfb54f73))
* force `prevrandao` on Moonbeam networks ([#9489](https://github.com/matter-labs/foundry-zksync/issues/9489)) ([c161c7c](https://github.com/matter-labs/foundry-zksync/commit/c161c7c9ed5f939adca5e88ff279654ae37c4a3d))
* **forge create:** install missing deps if any ([#9401](https://github.com/matter-labs/foundry-zksync/issues/9401)) ([66228e4](https://github.com/matter-labs/foundry-zksync/commit/66228e443846127499374d997aa5df9c898d4f5d))
* **forge create:** set skip_is_verified_check: true ([#9222](https://github.com/matter-labs/foundry-zksync/issues/9222)) ([dd443c6](https://github.com/matter-labs/foundry-zksync/commit/dd443c6c0b017718a97a2302328e61f5c01582c2))
* **forge doc:** display custom natspec tag ([32f8e79](https://github.com/matter-labs/foundry-zksync/commit/32f8e798298443565c789883206bd024b46c4712))
* **forge eip712:** handle recursive types ([a65a5b1](https://github.com/matter-labs/foundry-zksync/commit/a65a5b1445ba7ec9b10baf7ecb28f7a65bbb13ce))
* forge script should adhere to `--json` flag ([#9404](https://github.com/matter-labs/foundry-zksync/issues/9404)) ([0045384](https://github.com/matter-labs/foundry-zksync/commit/0045384f1087897b2665506e95808f022776a5a7))
* **forge test:** install missing dependencies before creating `Project` ([#9379](https://github.com/matter-labs/foundry-zksync/issues/9379)) ([76a2cb0](https://github.com/matter-labs/foundry-zksync/commit/76a2cb0dd6d60684fd64a8180500f9d619ec94d2))
* **forge:** add logs/decoded logs in json test results ([#9074](https://github.com/matter-labs/foundry-zksync/issues/9074)) ([a96b826](https://github.com/matter-labs/foundry-zksync/commit/a96b8266cf1f11e08ef0dfca9325ea6560d17c55))
* **forge:** always report deployment size in gas reports ([#9308](https://github.com/matter-labs/foundry-zksync/issues/9308)) ([54ea38d](https://github.com/matter-labs/foundry-zksync/commit/54ea38d189bf192f689aed4c6f231a27f1def316))
* **forge:** fix stack overflow when the lib path is absolute. ([#9190](https://github.com/matter-labs/foundry-zksync/issues/9190)) ([bcdd514](https://github.com/matter-labs/foundry-zksync/commit/bcdd514a633e27c29d5c00355311f6432cf31e8a))
* **forge:** generate `evm.legacyAssembly` extra output ([#8987](https://github.com/matter-labs/foundry-zksync/issues/8987)) ([4bcb309](https://github.com/matter-labs/foundry-zksync/commit/4bcb309eb8eb49e0033d58cce86bd31d44d7937a))
* **forge:** improve `test --debug` doc ([#8918](https://github.com/matter-labs/foundry-zksync/issues/8918)) ([90541f0](https://github.com/matter-labs/foundry-zksync/commit/90541f054f1666547a4869eed74751a7463b8571))
* **forge:** include legacyAssembly output ([4bcb309](https://github.com/matter-labs/foundry-zksync/commit/4bcb309eb8eb49e0033d58cce86bd31d44d7937a))
* fork reset ([#857](https://github.com/matter-labs/foundry-zksync/issues/857)) ([2179ac1](https://github.com/matter-labs/foundry-zksync/commit/2179ac163cef464666ea45243adcbc7d40431058))
* **fork:** set block blob_excess_gas_and_price only if `excess_blob_gas header` is Some ([#9298](https://github.com/matter-labs/foundry-zksync/issues/9298)) ([4817280](https://github.com/matter-labs/foundry-zksync/commit/4817280d96e0e33a2e96cf169770da60514d1764))
* **fork:** set block blob_excess_gas_and_price only if excess_blob_gas header is Some ([4817280](https://github.com/matter-labs/foundry-zksync/commit/4817280d96e0e33a2e96cf169770da60514d1764))
* **fuzz:** exclude external libraries addresses from fuzz inputs ([#9527](https://github.com/matter-labs/foundry-zksync/issues/9527)) ([59f354c](https://github.com/matter-labs/foundry-zksync/commit/59f354c179f4e7f6d7292acb3d068815c79286d1))
* gas price estimation cast send zk ([#683](https://github.com/matter-labs/foundry-zksync/issues/683)) ([c129b59](https://github.com/matter-labs/foundry-zksync/commit/c129b5911e3d838904ca7b6776278bd9bdffeda9))
* handle large years ([#9032](https://github.com/matter-labs/foundry-zksync/issues/9032)) ([eb04665](https://github.com/matter-labs/foundry-zksync/commit/eb046653de4047a27b181394338732e597965257))
* identification of contracts in scripts ([#9346](https://github.com/matter-labs/foundry-zksync/issues/9346)) ([c13d42e](https://github.com/matter-labs/foundry-zksync/commit/c13d42e850da353c0856a8b0d4123e13cc40045d))
* include `traces` field when running `forge test -vvvv --json` ([#9034](https://github.com/matter-labs/foundry-zksync/issues/9034)) ([22a72d5](https://github.com/matter-labs/foundry-zksync/commit/22a72d50aed05f5828655df2f29a1f8bab361653))
* include withdrawals root in response ([#9208](https://github.com/matter-labs/foundry-zksync/issues/9208)) ([3b0c75d](https://github.com/matter-labs/foundry-zksync/commit/3b0c75d5edd01e7be921b48b2e16271a467c2ffd))
* install libssl-dev for aarch64 binaries ([#858](https://github.com/matter-labs/foundry-zksync/issues/858)) ([fea525e](https://github.com/matter-labs/foundry-zksync/commit/fea525e471b46fd3964370bb285122afb0b9973e))
* **invariant:** do not commit state if assume returns ([#9062](https://github.com/matter-labs/foundry-zksync/issues/9062)) ([a17869a](https://github.com/matter-labs/foundry-zksync/commit/a17869a6dcce7ce3765c5ed521d40ddb572de9f0))
* linux installation of anvil-zksync and add ci check ([#780](https://github.com/matter-labs/foundry-zksync/issues/780)) ([e899df9](https://github.com/matter-labs/foundry-zksync/commit/e899df9488974fe8f4192dcf7985c770f32871e1))
* mark flag incompatibility  ([#9530](https://github.com/matter-labs/foundry-zksync/issues/9530)) ([91030da](https://github.com/matter-labs/foundry-zksync/commit/91030daee6e622dce6dd725fd4c48bcd36a54f46))
* merge journaled state during forks ([#705](https://github.com/matter-labs/foundry-zksync/issues/705)) ([c8c1e14](https://github.com/matter-labs/foundry-zksync/commit/c8c1e14ca0856fabc0b6750b3dbe31a33db0d24c))
* Nonce mismatches what network expects ([#726](https://github.com/matter-labs/foundry-zksync/issues/726)) ([f83f08e](https://github.com/matter-labs/foundry-zksync/commit/f83f08eb45dbf28b12437544722a6f2f0e7fa407))
* normalize EVM version in chisel ([#9040](https://github.com/matter-labs/foundry-zksync/issues/9040)) ([8905af3](https://github.com/matter-labs/foundry-zksync/commit/8905af382e04b1bf3a492880abe5904a56e88491))
* only test --eof on linux ([c89a08c](https://github.com/matter-labs/foundry-zksync/commit/c89a08c5b0bee69c8b6072853f0a34babbefc495))
* only test `--eof` on linux ([#9016](https://github.com/matter-labs/foundry-zksync/issues/9016)) ([c89a08c](https://github.com/matter-labs/foundry-zksync/commit/c89a08c5b0bee69c8b6072853f0a34babbefc495))
* Prepare tx transaction with right type ([#783](https://github.com/matter-labs/foundry-zksync/issues/783)) ([699f8e8](https://github.com/matter-labs/foundry-zksync/commit/699f8e8936a89b8b4396533d2e0658bf6882069d))
* redact RPC URLs in traces if URL is passed in directly ([#9077](https://github.com/matter-labs/foundry-zksync/issues/9077)) ([1465e39](https://github.com/matter-labs/foundry-zksync/commit/1465e39f853a7c7a151609cb3abe5dc19c52a94b))
* **remappings:** check if remapping to add starts with existing remapping name ([#9246](https://github.com/matter-labs/foundry-zksync/issues/9246)) ([455ba9b](https://github.com/matter-labs/foundry-zksync/commit/455ba9b1b736766232d84ba1790ac9ba6ca944de))
* **remappings:** ignore remappings of root proj dirs when merging ([#9258](https://github.com/matter-labs/foundry-zksync/issues/9258)) ([10a8e88](https://github.com/matter-labs/foundry-zksync/commit/10a8e8862ca5f9a28edebd9603f985349f536587))
* **remappings:** project autoremappings should respect config ([#9466](https://github.com/matter-labs/foundry-zksync/issues/9466)) ([25c978a](https://github.com/matter-labs/foundry-zksync/commit/25c978ae29454454cec857de3400a885efc4bd7c))
* remove duplicate `gas_limit` / `block_gas_limit` field, declare as alias ([#9406](https://github.com/matter-labs/foundry-zksync/issues/9406)) ([de5e89c](https://github.com/matter-labs/foundry-zksync/commit/de5e89cd117bb30f147c28862c51be6ef239f23f))
* Remove duplicated line ([#707](https://github.com/matter-labs/foundry-zksync/issues/707)) ([5d4bdb6](https://github.com/matter-labs/foundry-zksync/commit/5d4bdb64cf1201fdb774038a58b42f7d0454187b))
* remove macos from ci-install-anvil-zksync-check ([#786](https://github.com/matter-labs/foundry-zksync/issues/786)) ([4a81422](https://github.com/matter-labs/foundry-zksync/commit/4a814225db4ee327dc170093056396761cac1b73))
* remove steps after steps tracing cheatcodes are done ([#9234](https://github.com/matter-labs/foundry-zksync/issues/9234)) ([213d817](https://github.com/matter-labs/foundry-zksync/commit/213d8174727023cf2881825e4b4f9417d726e1c8))
* removes avoid-contracts in favour of --skip ([#702](https://github.com/matter-labs/foundry-zksync/issues/702)) ([772d1d8](https://github.com/matter-labs/foundry-zksync/commit/772d1d8e994531e88b7d2e4852e847649b47ebb1))
* rename flag as_int -&gt; as-int ([#9235](https://github.com/matter-labs/foundry-zksync/issues/9235)) ([9d74675](https://github.com/matter-labs/foundry-zksync/commit/9d74675bae8bfbd83428ff1343cbe2ae206c3383))
* reset shell colors based on the input style ([#9243](https://github.com/matter-labs/foundry-zksync/issues/9243)) ([7587eb5](https://github.com/matter-labs/foundry-zksync/commit/7587eb53a996ff289de2c8fdb4a49c93e90d5f9b))
* resolves issues with installing anvil-zksync with rosetta ([#820](https://github.com/matter-labs/foundry-zksync/issues/820)) ([e9562f3](https://github.com/matter-labs/foundry-zksync/commit/e9562f34e33cfb470bb931357c0c2edf48ceea82))
* respect `--auth` in `cast call` and `cast estimate` ([#9120](https://github.com/matter-labs/foundry-zksync/issues/9120)) ([cc8e430](https://github.com/matter-labs/foundry-zksync/commit/cc8e430cc9ad743265d8c897b855809128798d8f))
* Respect priority fee and max fee per gas for broadcasting txs with scripts ([#841](https://github.com/matter-labs/foundry-zksync/issues/841)) ([4a216e9](https://github.com/matter-labs/foundry-zksync/commit/4a216e99dc5aa3e6c8d02f206fcf2e3302ba767a))
* restore lock version 3 ([#9501](https://github.com/matter-labs/foundry-zksync/issues/9501)) ([92cd165](https://github.com/matter-labs/foundry-zksync/commit/92cd1650cedfe64b0985e224fcba7ebac38ba382))
* retrieve zks_getBytecodeByHash from SharedBackend when forking ([#815](https://github.com/matter-labs/foundry-zksync/issues/815)) ([b667ae0](https://github.com/matter-labs/foundry-zksync/commit/b667ae008b0e14c408d1858efbd13ded49b9f1fa))
* reverts changes to release from upstream merge ([#634](https://github.com/matter-labs/foundry-zksync/issues/634)) ([0b24fcc](https://github.com/matter-labs/foundry-zksync/commit/0b24fcc33b6eeaa837c1186366d473df9b527308))
* running script with `--broadcast` for a transaction sequence can error out due to nonce desync from rpc latency ([#9096](https://github.com/matter-labs/foundry-zksync/issues/9096)) ([6f7c1f7](https://github.com/matter-labs/foundry-zksync/commit/6f7c1f72f8c3361f1e738296a0ec634c099c8a7c))
* sanitize input based on solc version ([#690](https://github.com/matter-labs/foundry-zksync/issues/690)) ([883034d](https://github.com/matter-labs/foundry-zksync/commit/883034d4851db9d58a91d54953aeb62c60890243))
* sanitize yul artifact contract names ([#819](https://github.com/matter-labs/foundry-zksync/issues/819)) ([31d3744](https://github.com/matter-labs/foundry-zksync/commit/31d3744e7cbac3e88dd0bd8bac5fb5d42fff33df))
* script simulation with default sender ([#9042](https://github.com/matter-labs/foundry-zksync/issues/9042)) ([09824ad](https://github.com/matter-labs/foundry-zksync/commit/09824ad0cdb4d20e280e1698ca9097b869b2a4da))
* **script:** correctly detect additional contracts ([#9207](https://github.com/matter-labs/foundry-zksync/issues/9207)) ([513ed69](https://github.com/matter-labs/foundry-zksync/commit/513ed69f79cbc24cfc08d5ef39e9f8bb5fe7eff7))
* set user-agent header in runtime transport ([#9434](https://github.com/matter-labs/foundry-zksync/issues/9434)) ([4527475](https://github.com/matter-labs/foundry-zksync/commit/4527475bc8be4044a8daa1dddecb4086403c5b76))
* skip zk tests for upstream test job ([#824](https://github.com/matter-labs/foundry-zksync/issues/824)) ([74d2079](https://github.com/matter-labs/foundry-zksync/commit/74d207968f3c556bc9794a1ea3dace9eff45ab78))
* support EOF opcodes in `cast da` ([#9070](https://github.com/matter-labs/foundry-zksync/issues/9070)) ([ad86979](https://github.com/matter-labs/foundry-zksync/commit/ad86979e06c0577fc097577358e460e7f5ec9bdf))
* **trace:** check fn sigs for contract with fallbacks ([#9287](https://github.com/matter-labs/foundry-zksync/issues/9287)) ([e028b92](https://github.com/matter-labs/foundry-zksync/commit/e028b92698eae7e5019025e1784e7c06c3cae534))
* **traces:** identify artifacts using both deployed and creation code ([#9050](https://github.com/matter-labs/foundry-zksync/issues/9050)) ([fdd321b](https://github.com/matter-labs/foundry-zksync/commit/fdd321bac95f0935529164a88faf99d4d5cfa321))
* update era-deps to avoid BytecodeCompression error ([#636](https://github.com/matter-labs/foundry-zksync/issues/636)) ([658ee70](https://github.com/matter-labs/foundry-zksync/commit/658ee7060fd59b3533728e2138d23e556a18233e))
* update foundry-compilers ([#743](https://github.com/matter-labs/foundry-zksync/issues/743)) ([825279c](https://github.com/matter-labs/foundry-zksync/commit/825279cf91b5bfb3e5bf28f9745096a7326027e9))
* use `Debug` when formatting errors ([#9251](https://github.com/matter-labs/foundry-zksync/issues/9251)) ([8660e5b](https://github.com/matter-labs/foundry-zksync/commit/8660e5b941fe7f4d67e246cfd3dafea330fb53b1))
* use cross for builds ([#860](https://github.com/matter-labs/foundry-zksync/issues/860)) ([87e8c5e](https://github.com/matter-labs/foundry-zksync/commit/87e8c5e0f1ad0affa97475cce4bd5feaa1db6944))
* use dedicated gas limit for each tx in inspect_batch ([f6b5d08](https://github.com/matter-labs/foundry-zksync/commit/f6b5d081f9f849721798497c1982558babb4ec8b))
* use regular `println` in internal test utils to avoid interfering with `cargo test` runner ([#9296](https://github.com/matter-labs/foundry-zksync/issues/9296)) ([b7fe62e](https://github.com/matter-labs/foundry-zksync/commit/b7fe62ef1f58bfa2fe1980cc0f065dfc48b31d30))
* use zksync_deploy method to set correct tx params ([9c60954](https://github.com/matter-labs/foundry-zksync/commit/9c609545112fd769005da14ebfbccac01518507c))
* **verify:** cached artifacts by version ([#9520](https://github.com/matter-labs/foundry-zksync/issues/9520)) ([2e56b8f](https://github.com/matter-labs/foundry-zksync/commit/2e56b8f63beeffab36d8c6f8b7563b9e92601f71))
* **verify:** set skip_is_verified_check: true for deploy (similar to broadcast) ([dd443c6](https://github.com/matter-labs/foundry-zksync/commit/dd443c6c0b017718a97a2302328e61f5c01582c2))
* vm.broadcastRawTransaction ([2bc7125](https://github.com/matter-labs/foundry-zksync/commit/2bc7125e913b211b2d6c59ecdc5f1f427440652b))
* **zk:** invariant testing ([#581](https://github.com/matter-labs/foundry-zksync/issues/581)) ([e2e2d57](https://github.com/matter-labs/foundry-zksync/commit/e2e2d57197cbdd5fac7fc3c197becb8584780d59))


### Performance Improvements

* cap default poll interval ([#9250](https://github.com/matter-labs/foundry-zksync/issues/9250)) ([97be9b9](https://github.com/matter-labs/foundry-zksync/commit/97be9b9a2e128633b17589cd58bfde4b4d544e23))
* **coverage:** cache computed bytecode hash in CoverageCollector ([#9457](https://github.com/matter-labs/foundry-zksync/issues/9457)) ([d35fee6](https://github.com/matter-labs/foundry-zksync/commit/d35fee62382b9bf66c946f3f9b6646e00a64db43))
* **coverage:** cache current HitMap, reserve when merging ([#9469](https://github.com/matter-labs/foundry-zksync/issues/9469)) ([22202a7](https://github.com/matter-labs/foundry-zksync/commit/22202a7a2b3abed5ff74a226dfed790197ac7723))
* **coverage:** improve HitMap merging and internal repr ([#9456](https://github.com/matter-labs/foundry-zksync/issues/9456)) ([b7a065f](https://github.com/matter-labs/foundry-zksync/commit/b7a065f79fa63c80ece43e05b5e521ae269b4635))
* reduce dynamic dispatch for inspectors ([#9011](https://github.com/matter-labs/foundry-zksync/issues/9011)) ([df2e91b](https://github.com/matter-labs/foundry-zksync/commit/df2e91b5e22a9ebce2924f0f56c54508d36f1241))

## Changelog

## Pre 1.0

### Important note for users

Multiple breaking changes will occur so Semver can be followed as soon as Foundry 1.0 is released. They will be listed here, along with the updates needed for your projects.

If you need a stable Foundry version, we recommend using the latest pinned nightly of May 2nd, locally and on your CI.

To use the latest pinned nightly locally, use the following command:

```
foundryup --version nightly-e15e33a07c0920189fc336391f538c3dad53da73
````

To use the latest pinned nightly on your CI, modify your Foundry installation step to use an specific version:

```
- name: Install Foundry
  uses: foundry-rs/foundry-toolchain@v1
  with:
    version: nightly-e15e33a07c0920189fc336391f538c3dad53da73
```

### Breaking changes

- [expectEmit](https://github.com/foundry-rs/foundry/pull/4920) will now only work for the next call.
- expectCall will now only work if the call(s) are made exactly after the cheatcode is invoked.
- [expectRevert will now work if the next call does revert](https://github.com/foundry-rs/foundry/pull/4945), instead of expecting a revert during the whole test.
  - This will very likely break your tests. Please make sure that all the calls you expect to revert are external, and if not, abstract them into a separate contract so that they can be called externally and the cheatcode can be used.
- `-m`, the deprecated alias for `--mt` or `--match-test`, has now been removed.
- [startPrank will now override the existing prank instead of erroring](https://github.com/foundry-rs/foundry/pull/4826).
- [precompiles will not be compatible with all cheatcodes](https://github.com/foundry-rs/foundry/pull/4905).
- The difficulty and prevrandao cheatcodes now [fail if not used with the correct EVM version](https://github.com/foundry-rs/foundry/pull/4904).
- The default EVM version will be Shanghai. If you're using an EVM chain which is not compatible with [EIP-3855](https://eips.ethereum.org/EIPS/eip-3855) you need to change your EVM version. See [Matt Solomon's thread](https://twitter.com/msolomon44/status/1656411871635972096) for more information.
- Non-existent JSON keys are now processed correctly, and `parseJson` returns non-decodable empty bytes if they do not exist. https://github.com/foundry-rs/foundry/pull/5511
