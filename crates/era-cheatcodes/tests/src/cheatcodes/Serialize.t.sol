// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";
import {Utils} from "./Utils.sol";

contract CheatcodeSerializeTest is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;

    function testSerializeAddress() external {
        string memory testString = vm.serializeAddress("obj1", "address", TEST_ADDRESS);
        require(
            keccak256(bytes(testString)) ==
                keccak256(bytes("0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a")),
            "serializeAddress mismatch"
        );
    }

    function testSerializeBool() external {
        string memory testString = vm.serializeBool("obj1", "boolean", true);
        require(
            keccak256(bytes(testString)) == keccak256(bytes("true")),
            "serializeBool mismatch"
        );
    }

    function testSerializeUint() external {
        string memory testString = vm.serializeUint("obj1", "uint", 99);
        require(
            keccak256(bytes(testString)) == keccak256(bytes("99")),
            "serializeUint mismatch"
        );
    }
}
