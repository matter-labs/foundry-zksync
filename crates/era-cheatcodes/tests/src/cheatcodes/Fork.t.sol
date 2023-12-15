
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract ForkTest is Test {
    address constant TEST_ADDRESS = 0x10b252872733BFdC7fB22dB0BE5D1E55C0141848;
    address constant TOKEN_ADDRESS = 0x493257fD37EDB34451f62EDf8D2a0C418852bA4C;
    function setUp() public {
        (bool success, bytes memory data) = TOKEN_ADDRESS.call(
            abi.encodeWithSignature("getBalance(address)", TEST_ADDRESS)
        );
        console.log("balance before:", uint256(bytes32(data)));
         (bool success1, ) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("createSelectFork(string,uint256)", "mainnet", 243698)
        );
        require(success1, "fork failed");   
    }
    function testFork() public {

        // (bool success2, bytes memory data2) = TOKEN_ADDRESS.call(
        //     abi.encodeWithSignature("getBalance(address)", TEST_ADDRESS)
        // );
        // let balance_after = uint256(bytes32(data2));
        // require(success2, "balance failed");   
        uint256 balance_after = 1;
        console.log("balance after:", balance_after);

    }
}
