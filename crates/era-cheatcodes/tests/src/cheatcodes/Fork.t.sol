
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract ForkTest is Test {
    /// USDC TOKEN 
    address constant TOKEN_ADDRESS = 0x3355df6D4c9C3035724Fd0e3914dE96A5a83aaf4;
    uint256 constant TOKEN_DECIMALS = 6;
    uint256 constant FORK_BLOCK = 19579636;
    function setUp() public {
        /// USDC TOKEN doesn't exists locally
        (bool success, bytes memory data) = TOKEN_ADDRESS.call(
            abi.encodeWithSignature("decimals()")
        );
        uint256 decimals_before = uint256(bytes32(data));
        require(block.number < 1000, "Local node doesn't have blocks above 1000");
         (bool success1, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("createSelectFork(string,uint256)", "mainnet", FORK_BLOCK)
        );
        require(decimals_before == 0, "Contract exists locally");
        require(success1, "fork failed");   
    } 

    function testFork() public{
        /// After createSelect fork the decimals  should exist
        (bool success2, bytes memory data2) = TOKEN_ADDRESS.call(
            abi.encodeWithSignature("decimals()")
        );
        uint256 decimals_after = uint256(bytes32(data2));
        console.log("decimals_after", decimals_after);  
        require(decimals_after == TOKEN_DECIMALS, "Contract dosent exists in fork");
        require(block.number == FORK_BLOCK + 1, "ENV for blocks is not set correctly");

    }

    function testCreateSelectFork() public{
        (bool success, bytes memory data) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("createFork(string,uint256)", "mainnet", FORK_BLOCK + 100)
        );
        require(success, "fork failed");

        uint256 forkId = uint256(bytes32(data));
        (bool success1, bytes memory data1) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("selectFork(uint256)", forkId)
        );
        require(success1, "select fork failed");


        /// After createSelect fork the decimals  should exist
        (bool success2, bytes memory data2) = TOKEN_ADDRESS.call(
            abi.encodeWithSignature("decimals()")
        );
        uint256 decimals_after = uint256(bytes32(data2));
        console.log("decimals_after", decimals_after);  
        console.log("block ", block.number);  
        require(decimals_after == TOKEN_DECIMALS, "Contract dosent exists in fork");
        require(block.number == FORK_BLOCK + 100, "ENV for blocks is not set correctly");
    }
}
