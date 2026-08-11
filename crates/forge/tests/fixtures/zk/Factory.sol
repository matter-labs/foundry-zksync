// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

import "zksync-contracts/zksync-contracts/l2/system-contracts/Constants.sol";
import "zksync-contracts/zksync-contracts/l2/system-contracts/libraries/SystemContractsCaller.sol";

contract Factory {
    bytes32 public bytecodeHash;

    constructor(bytes32 _bytecodeHash) {
        bytecodeHash = _bytecodeHash;
    }

    function deployContract(
        bytes32 salt,
        bytes memory constructorArgs
    ) external returns (address contractAddress) {
        (bool success, bytes memory returnData) = SystemContractsCaller
            .systemCallWithReturndata(
                uint32(gasleft()),
                address(DEPLOYER_SYSTEM_CONTRACT),
                uint128(0),
                abi.encodeCall(
                    DEPLOYER_SYSTEM_CONTRACT.create2,
                    (salt, bytecodeHash, constructorArgs)
                )
            );

        require(success, "Deployment failed");

        (contractAddress) = abi.decode(returnData, (address));
    }
}
