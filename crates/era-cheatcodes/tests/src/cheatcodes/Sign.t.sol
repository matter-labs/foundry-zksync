// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract SignTest is Test {
    function test_Sign() public {
        (address alice, uint256 alicePk) = makeAddrAndKey("alice");
        bytes32 message = "hello world";
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(alicePk, message);

        address signer = ecrecover(keccak256(abi.encodePacked(message)), v, r, s);
        assertEq(alice, signer); // [PASS]
    }
}
