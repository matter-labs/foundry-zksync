
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract ForkTest is Test {
    address constant TEST_ADDRESS = 0x10b252872733BFdC7fB22dB0BE5D1E55C0141848;
    function testFork() public {
        uint256 balanceBefore = address(TEST_ADDRESS).balance;
        console.log("balance before:", balanceBefore);
         (bool success, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("createSelectFork(string,uint256)", "mainnet", 243698)
        );
        require(success, "fork failed");   
        uint256 balanceAfter = address(TEST_ADDRESS).balance;
        console.log("balance after :", balanceAfter);
    }
}