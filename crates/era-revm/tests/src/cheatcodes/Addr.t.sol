// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract AddrTest is Test {
    function testAddr() public {
        uint256 pk = 77814517325470205911140941194401928579557062014761831930645393041380819009408;
        address expected = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;

        (bool success, bytes memory data) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature("addr(uint256)", pk)
        );
        require(success, "addr failed");
        address addr = abi.decode(data, (address));
        assertEq(addr, expected, "expected address did not match");
        console.log("failed?", failed());
    }
}