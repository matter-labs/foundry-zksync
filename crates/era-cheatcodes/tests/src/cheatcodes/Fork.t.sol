
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract ForkTest is Test {
    /// USDC TOKEN 
    address constant TOKEN_ADDRESS = 0x3355df6D4c9C3035724Fd0e3914dE96A5a83aaf4;
    function setUp() public {
        /// USDC TOKEN doesn't exists locally
        (bool success, bytes memory data) = TOKEN_ADDRESS.call(
            abi.encodeWithSignature("decimals()")
        );
        uint256 decimals_before = uint256(bytes32(data));
        console.log("decimals_before:", decimals_before);
         (bool success1, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("createSelectFork(string,uint256)", "mainnet", 243698)
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
        console.log("decimals_after:", decimals_after);
        require(decimals_after == 6, "Contract dosent exists in fork");

    }
}
