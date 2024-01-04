// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";
import {Counter} from "./Counter.sol";

contract Create2Test is Test {
    Counter internal counter;

    function setUp() public {
        counter = new Counter();
    }

    function testDeterministicDeploy() public {
        // Bytecode hash for Counter.sol
        // We are hardcoding this since using `type(Counter).creationCode` returns the stored
        // bytecodeHash with 0 padding instead of the actual bytecode stored for the contract.
        // The actual bytecode is stored in factory deps so is unavailable.
        bytes32 codeHash = 0x0100001936df07a6c3d4492da5abea4e1b3fb7e6a07cfeaa938af4d12366b59b;
        bytes memory expectedCreationCode = bytes.concat(
            bytes4(0x0000),
            bytes32(
                0x0000000000000000000000000000000000000000000000000000000000000000
            ),
            codeHash,
            bytes32(
                0x0000000000000000000000000000000000000000000000000000000000000000
            ),
            bytes32(
                0x0000000000000000000000000000000000000000000000000000000000000000
            )
        );
        require(
            keccak256(type(Counter).creationCode) ==
                keccak256(expectedCreationCode),
            "Counter.sol bytecode mismatch"
        );

        address sender = address(this);
        bytes32 salt = "12345";
        bytes32 constructorInputHash = keccak256(abi.encode());
        address expectedDeployedAddress = computeCreate2Address(
            sender,
            salt,
            codeHash,
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
