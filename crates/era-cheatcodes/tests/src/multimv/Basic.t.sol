// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console, Vm} from "../../lib/forge-std/src/Test.sol";

contract MultiVMBasicTest is Test {
    /// USDC TOKEN
    uint256 constant TOKEN_DECIMALS = 6;
    address constant ERA_TOKEN_ADDRESS = 0x3355df6D4c9C3035724Fd0e3914dE96A5a83aaf4;
    uint256 constant ERA_FORK_BLOCK = 9350;

    address constant ETH_TOKEN_ADDRESS = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48;
    uint256 constant ETH_FORK_BLOCK = 19191416;

    uint256 forkEra;
    uint256 forkEth;

    function _smokeSetUp() public {
        /// USDC TOKEN doesn't exists locally
        (bool success, bytes memory data) = ERA_TOKEN_ADDRESS.call(
            abi.encodeWithSignature("decimals()")
        );
        require(success, "decimals() failed");
        uint256 decimals_before = uint256(bytes32(data));
        require(
            block.number < 1000,
            "Local node doesn't have blocks above 1000"
        );

        //for now, createSelect = zkSync and create = Eth
        //after we put the right logic in place we can change this all to createFork only
        forkEra = vm.createSelectFork("mainnet", ERA_FORK_BLOCK);
        forkEth = vm.createFork("https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf", ETH_FORK_BLOCK);
    }

    function _checkToken(address token, uint256 blockNum) public {
        (bool success, bytes memory data2) = token.call(
            abi.encodeWithSignature("decimals()")
        );
        require(success, "decimals() failed");
        uint256 decimals_after = uint256(bytes32(data2));
        require(
            decimals_after == TOKEN_DECIMALS,
            "Contract doesn't exists in fork"
        );
    }

    function testSmoke() public {
       _smokeSetUp();

       //check that we are on zkSync mainnet
       vm.selectFork(forkEra);
       _checkToken(ERA_TOKEN_ADDRESS, ERA_FORK_BLOCK);


       //check that we are on eth mainnet
        vm.selectFork(forkEth);
       _checkToken(ETH_TOKEN_ADDRESS, ETH_FORK_BLOCK);
    }
}
