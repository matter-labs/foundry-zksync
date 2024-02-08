# 🔧 Supported Cheatcodes for Foundry-zksync 🔧

> ⚠️ **WORK IN PROGRESS**: This list is non-comprehensive and being updated. If there is an cheatcode that requires additional support, please start by [creating a GitHub Issue](https://github.com/matter-labs/foundry-zksync/issues/new/choose).

## Key

The `status` options are:

+ `SUPPORTED` - Basic support is completed
+ `NOT IMPLEMENTED` - Currently not supported/implemented

## Supported Cheatcodes Table

| Cheatcode | Status | Link |
| --- | --- | --- |
| `vm.setNonce` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/set-nonce) |
| `vm.getNonce` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/get-nonce) |
| `vm.deal` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/deal) |
| `vm.etch` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/etch) |
| `vm.warp(u256)` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/warp) |
| `vm.roll` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/roll) |
| `vm.startPrank` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/start-prank) |
| `vm.stopPrank` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/stop-prank) |
| `vm.addr` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/addr) |
| `vm.toString` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/to-string) |
| `vm.readCallers` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/read-callers) |
| `vm.expectRevert` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/expect-revert) |
| `vm.recordLogs` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/record-logs) |
| `vm.getRecordedLogs` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/get-recorded-logs) |
| `vm.snapshot` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/snapshots) |
| `vm.revertTo` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/snapshots?highlight=revertTo#signature) |
| `vm.expectEmit` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/expect-emit) |
| `vm.expectCall` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/expect-call) |
| `vm.createFork` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/create-fork) |
| `vm.selectFork` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/select-fork) |
| `vm.createSelectFork` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/create-select-fork) |
| `vm.rpcUrl` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/rpc) |
| `vm.activeFork` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/active-fork) |
| `vm.writeFile` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/fs) |
| `vm.writeJson` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/fs) |
| `vm.serializeUint` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/serialize-json?highlight=serializeUint#signature) |
| `vm.serializeAddress` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/serialize-json?highlight=serializeAddress#signature) |
| `vm.serializeBool` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/serialize-json?highlight=serializeBool#signature) |
| `vm.store` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/store) |
| `vm.load` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/load) |
| `vm.ffi` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/ffi) |
| `vm.tryFfi` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/ffi) |
| `vm.startBroadcast` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/start-broadcast) |
| `vm.stopBroadcast` | SUPPORTED | [Link](https://book.getfoundry.sh/cheatcodes/stop-broadcast) |
| `vm.sign`              | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/sign)          |
| `vm.setEnv`            | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/set-env.html)  |
| `vm.transact`          | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/transact.html) |
| `vm.makePersistance`   | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/make-persistent.html) |
| `vm.revokePersistance` | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/revoke-persistent.html) |
| `vm.isPersistent`      | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/is-persistent.html) |
| `vm.rollFork`          | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/roll-fork.html) |
| `vm.assume`            | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/assume)        |
| `vm.mockCall`          | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/mock-call)        |
| `vm.clearMockedCall`   | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/clear-mocked-calls#clearmockedcalls)        |
| `vm.envUint`           | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/env-uint)      |
| `vm.envBool`           | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/env-bool)      |
| `vm.envInt`            | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/env-int)      |
| `vm.envAddress`        | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/env-address)      |
| `vm.envBytes32`        | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/env-bytes32)      |
| `vm.envString`         | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/env-string)     |
| `vm.envBytes`          | SUPPORTED  | [Link](https://book.getfoundry.sh/cheatcodes/env-bytes)      |
