// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

library Utils {
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
