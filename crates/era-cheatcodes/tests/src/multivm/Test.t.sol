// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";


contract Number {
    function ten() public pure returns (uint8) {
        return 10;
    }
}


contract FooTest is Test {
    Number number;

    /// USDC TOKEN
    uint256 constant TOKEN_DECIMALS = 6;

    address constant ERA_TOKEN_ADDRESS =
        0x3355df6D4c9C3035724Fd0e3914dE96A5a83aaf4;
    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    address constant ETH_TOKEN_ADDRESS =
        0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48;
    uint256 constant ETH_FORK_BLOCK = 19225195;
    uint256 constant ETH_FORK_BLOCK_TS = 1707901427;

    address constant CONTRACT_ADDRESS =
        0x32400084C286CF3E17e7B677ea9583e60a000324; //zkSync Diamond Proxy
    uint256 constant ERA_BALANCE = 372695034186505563;
    uint256 constant ETH_BALANCE = 153408823439331882193477;

    uint256 forkEra;
    uint256 forkEth;

    function setUp() public {
        number = new Number();
        forkEra = vm.createFork("local", ERA_FORK_BLOCK);
        forkEth = vm.createFork(
            "https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf",
            ETH_FORK_BLOCK
        );
    }


    function testFoo() public {
        vm.selectFork(forkEra);
        console.log("1");
        require(number.ten() == 10, "era setUp contract value mismatch");

        vm.selectFork(forkEth);
        console.log("2");
        require(number.ten() == 10, "eth setUp contract value mismatch");

        vm.selectFork(forkEra);
    }
}
