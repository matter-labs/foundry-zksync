// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";

contract EnvTest is Test {
    uint256 constant numEnvUintTests = 6;

    function testEnvUint() public {
        string memory key = "_foundryCheatcodeEnvUintTestKey";
        string[numEnvUintTests] memory values = [
            "0",
            "115792089237316195423570985008687907853269984665640564039457584007913129639935",
            "0x01",
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            "0x0000000000000000000000000000000000000000000000000000000000000000",
            "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"
        ];
        uint256[numEnvUintTests] memory expected = [
            type(uint256).min,
            type(uint256).max,
            1,
            77814517325470205911140941194401928579557062014761831930645393041380819009408,
            type(uint256).min,
            type(uint256).max
        ];
        for (uint256 i = 0; i < numEnvUintTests; ++i) {
            vm.setEnv(key, values[i]);
            uint256 output = vm.envUint(key);
            require(output == expected[i], "envUint failed");
        }
    }

    function testEnvUintArr() public {
        string memory key = "_foundryCheatcodeEnvUintArrTestKey";
        string memory value = "0,"
        "115792089237316195423570985008687907853269984665640564039457584007913129639935,"
        "0x0000000000000000000000000000000000000000000000000000000000000000,"
        "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF";
        uint256[4] memory expected = [
            type(uint256).min,
            type(uint256).max,
            type(uint256).min,
            type(uint256).max
        ];

        vm.setEnv(key, value);
        string memory delimiter = ",";
        uint256[] memory output = vm.envUint(key, delimiter);
        require(
            keccak256(abi.encodePacked((output))) ==
                keccak256(abi.encodePacked((expected))),
            "envUintArr failed"
        );
    }

    function testEnvUintEmptyArr() public {
        string memory key = "_foundryCheatcodeEnvUintEmptyArrTestKey";
        string memory value = "";
        uint256[] memory expected = new uint256[](0);

        vm.setEnv(key, value);
        string memory delimiter = ",";
        uint256[] memory output = vm.envUint(key, delimiter);
        require(
            keccak256(abi.encodePacked((output))) ==
                keccak256(abi.encodePacked((expected))),
            "envUintEmptyArr failed"
        );
    }
}
