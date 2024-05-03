// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";

contract Number {
    function ten() public pure returns (uint8) {
        return 10;
    }
}

contract ZkSetupForkFailureTest is Test {
    uint256 constant ETH_FORK_BLOCK = 18993187;
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;
    Number number;

    function setUp() public {
        vm.createSelectFork(
            "https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf",
            ETH_FORK_BLOCK
        );
        number = new Number();
    }

    function testFail_ZkSetupForkFailureExecutesTest() public pure {
        assert(false);
    }
}
