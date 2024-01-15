// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";
import {Counter} from "./Counter.sol";

contract Create2Test is Test {
    Counter internal counter;

    function setUp() public {
        counter = new Counter();
    }

    function testDeterministicDeploy() public {
        // Bytecode hash for Counter.sol needs to be parsed from the following format
        //   0x0000
        //   0x0000000000000000000000000000000000000000000000000000000000000000
        //   BYTE_CODE
        //   0x0000000000000000000000000000000000000000000000000000000000000000
        //   0x0000000000000000000000000000000000000000000000000000000000000000
        bytes memory creationCode = type(Counter).creationCode;
        bytes memory bytecodeHash = abi.encodePacked(
            bytes32(
                0x0000000000000000000000000000000000000000000000000000000000000000
            )
        );
        for (uint8 i = 0; i < 32; i++) {
            bytecodeHash[i] = creationCode[4 + 32 + i];
        }

        address sender = address(this);
        bytes32 salt = "12345";
        bytes32 constructorInputHash = keccak256(abi.encode());
        address expectedDeployedAddress = computeCreate2Address(
            sender,
            salt,
            bytes32(bytecodeHash),
            constructorInputHash
        );

        // deploy via create2
        address actualDeployedAddress = address(new Counter{salt: salt}());

        assertEq(expectedDeployedAddress, actualDeployedAddress);
    }

    function computeCreate2Address(
        address sender,
        bytes32 salt,
        bytes32 creationCodeHash,
        bytes32 constructorInputHash
    ) private pure returns (address) {
        bytes32 zksync_create2_prefix = keccak256("zksyncCreate2");
        bytes32 address_hash = keccak256(
            bytes.concat(
                zksync_create2_prefix,
                bytes32(uint256(uint160(sender))),
                salt,
                creationCodeHash,
                constructorInputHash
            )
        );

        return address(uint160(uint256(address_hash)));
    }
}
