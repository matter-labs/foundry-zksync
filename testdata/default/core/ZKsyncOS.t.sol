// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "utils/Test.sol";

contract ZKsyncOSTest is Test {
    address constant L2_BASE_TOKEN_ADDRESS = 0x000000000000000000000000000000000000800A;

    function testBalanceOf() public {
        vm.deal(address(1337), 10_000);

        (bool success, bytes memory retdata) = address(L2_BASE_TOKEN_ADDRESS).call(
            abi.encodeWithSignature("balanceOf(address)", address(1337))
        );
        require(success, "balanceOf call failed");
        uint256 balance = abi.decode(retdata, (uint256));

        assertEq(balance, 10_000, "balance mismatch");
    }
}
