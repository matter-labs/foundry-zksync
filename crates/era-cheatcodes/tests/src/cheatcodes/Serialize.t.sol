// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract CheatcodeSerializeTest is Test {
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;

    function testSerializeAddress() external {
        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "serializeAddress(string,string,address)",
                "obj1",
                "address",
                TEST_ADDRESS
            )
        );
        require(success, "serializeAddress failed");
        bytes memory data = trimReturnBytes(rawData);
        string memory testString = string(abi.encodePacked(data));
        require(
            keccak256(bytes(testString)) ==
                keccak256(bytes("0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a")),
            "serializeAddress mismatch"
        );
        console.log("failed?", failed());
    }

    function testSerializeBool() external {
        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "serializeBool(string,string,bool)",
                "obj1",
                "boolean",
                true
            )
        );
        require(success, "serializeBool failed");
        bytes memory data = trimReturnBytes(rawData);
        string memory testString = string(abi.encodePacked(data));
        require(
            keccak256(bytes(testString)) == keccak256(bytes("true")),
            "serializeBool mismatch"
        );
        console.log("failed?", failed());
    }

    function testSerializeUint() external {
        (bool success, bytes memory rawData) = Constants.CHEATCODE_ADDRESS.call(
            abi.encodeWithSignature(
                "serializeUint(string,string,uint256)",
                "obj1",
                "uint",
                99
            )
        );
        require(success, "serializeUint failed");
        bytes memory data = trimReturnBytes(rawData);
        string memory testString = string(abi.encodePacked(data));
        require(
            keccak256(bytes(testString)) == keccak256(bytes("99")),
            "serializeUint mismatch"
        );
        console.log("failed?", failed());
    }

    function trimReturnBytes(
        bytes memory rawData
    ) internal pure returns (bytes memory) {
        uint256 lengthStartingPos = rawData.length - 32;
        bytes memory lengthSlice = new bytes(32);
        for (uint256 i = 0; i < 32; i++) {
            lengthSlice[i] = rawData[lengthStartingPos + i];
        }
        uint256 length = abi.decode(lengthSlice, (uint256));
        bytes memory data = new bytes(length);
        for (uint256 i = 0; i < length; i++) {
            data[i] = rawData[i];
        }
        return data;
    }
}
