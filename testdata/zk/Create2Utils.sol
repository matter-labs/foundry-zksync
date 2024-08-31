// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.7 <0.9.0;

library Create2Utils {
    function computeCreate2Address(address sender, bytes32 salt, bytes32 creationCodeHash, bytes32 constructorInputHash)
        internal
        pure
        returns (address)
    {
        bytes32 zksync_create2_prefix = keccak256("zksyncCreate2");
        bytes32 address_hash = keccak256(
            bytes.concat(
                zksync_create2_prefix, bytes32(uint256(uint160(sender))), salt, creationCodeHash, constructorInputHash
            )
        );

        return address(uint160(uint256(address_hash)));
    }
}
