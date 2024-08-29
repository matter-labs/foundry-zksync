// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "forge-std/Test.sol";
import {L2ContractHelper} from "era-contracts/l2-contracts/contracts/L2ContractHelper.sol"; // =0.8.20

import {Greeter} from "../src/Greeter.sol";
import {CustomNumber} from "../src/CustomNumber.sol";

import {Create2Utils} from "../src/Create2Utils.sol";

contract Create2Test is Test {
    function getBytecodeHash(string memory path) internal returns (bytes32 bytecodeHash) {
        string memory artifact = vm.readFile(path);
        bytecodeHash = vm.parseJsonBytes32(artifact, ".hash");
    }

    function testCanDeployViaCreate2() public {
        bytes32 bytecodeHash = getBytecodeHash("zkout/Greeter.sol/Greeter.json");
        address sender = address(0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496);
        bytes32 salt = "12345";
        bytes32 constructorInputHash = keccak256(abi.encode());

        address computedAddress =
            Create2Utils.computeCreate2Address(sender, salt, bytes32(bytecodeHash), constructorInputHash);

        // deploy via create2
        address actualAddress = address(new Greeter{salt: salt}());

        assertEq(actualAddress, computedAddress);
    }

    function testComputeCreate2WithNoArgs() external {
        bytes32 salt = bytes32(0x0);

        bytes32 bytecodeHash = getBytecodeHash("zkout/Greeter.sol/Greeter.json");

        address computedAddress =
            Create2Utils.computeCreate2Address(address(this), salt, bytes32(bytecodeHash), keccak256(abi.encode()));
        address expectedAddress =
            L2ContractHelper.computeCreate2Address(address(this), salt, bytes32(bytecodeHash), keccak256(abi.encode()));

        address actualAddress = address(new Greeter{salt: salt}());
        assertEq(actualAddress, expectedAddress);
        assertEq(computedAddress, expectedAddress);
    }

    function testComputeCreate2WithArgs() external {
        bytes32 salt = bytes32(0x0);
        uint8 value = 42;

        bytes32 bytecodeHash = getBytecodeHash("zkout/CustomNumber.sol/CustomNumber.json");

        address computedAddress =
            Create2Utils.computeCreate2Address(address(this), salt, bytecodeHash, keccak256(abi.encode(value)));
        address expectedAddress =
            L2ContractHelper.computeCreate2Address(address(this), salt, bytecodeHash, keccak256(abi.encode(value)));

        CustomNumber num = new CustomNumber{salt: salt}(value);
        assertEq(address(num), expectedAddress);
        assertEq(computedAddress, expectedAddress);
        assertEq(num.number(), value);
    }
}
