// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

import "utils/Test.sol";

contract Issue1036 is Test {
    function test_forkSuceedsViaWebsocket() public {
        // The issue presented itself when using websocket endpoints and
        // not initializing properly the crypto provider.
        vm.createSelectFork("wss://mainnet.era.zksync.io/ws");
    }
}
