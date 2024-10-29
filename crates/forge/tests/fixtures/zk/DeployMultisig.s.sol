// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

import "forge-std/Script.sol";
import "zksync-contracts/zksync-contracts/l2/system-contracts/libraries/SystemContractsCaller.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "../src/AAFactory.sol";
import "../src/TwoUserMultisig.sol";

contract DeployMultisig is Script {
    function run() external {
        // Owners for the multisig account
        // Can be random
        address owner1 = makeAddr("OWNER_1");
        address owner2 = makeAddr("OWNER_2");

        // Read artifact file and get the bytecode hash
        string memory artifact = vm.readFile("zkout/TwoUserMultisig.sol/TwoUserMultisig.json");
        bytes32 multisigBytecodeHash = vm.parseJsonBytes32(artifact, ".hash");
        console.log("Bytecode hash: ");
        console.logBytes32(multisigBytecodeHash);
        bytes32 salt = "CEACREACREA";

        vm.startBroadcast();
        AAFactory factory = new AAFactory(multisigBytecodeHash);
        console.log("Factory deployed at: ", address(factory));
        (bool _success,) = address(vm).call(abi.encodeWithSignature("zkUseFactoryDep(string)", "TwoUserMultisig"));
        require(_success, "Deployment failed");
        factory.deployAccount(salt, owner1, owner2);
        string memory factoryArtifact = vm.readFile("zkout/AAFactory.sol/AAFactory.json");
        bytes32 factoryBytecodeHash = vm.parseJsonBytes32(factoryArtifact, ".hash");
        Create2Factory create2Factory = new Create2Factory();
        address multisigAddress = create2Factory.create2(salt, factoryBytecodeHash, abi.encode(owner1, owner2));
        console.log("Multisig deployed at: ", multisigAddress);

        vm.stopBroadcast();
    }
}

import {DEPLOYER_SYSTEM_CONTRACT} from "zksync-contracts/zksync-contracts/l2/system-contracts/Constants.sol";
import {EfficientCall} from "zksync-contracts/zksync-contracts/l2/system-contracts/libraries/EfficientCall.sol";
import {IContractDeployer} from "zksync-contracts/zksync-contracts/l2/system-contracts/interfaces/IContractDeployer.sol";

/// @custom:security-contact security@matterlabs.dev
/// @author Matter Labs
/// @notice The contract that can be used for deterministic contract deployment.
contract Create2Factory {
    /// @notice Function that calls the `create2` method of the `ContractDeployer` contract.
    /// @dev This function accepts the same parameters as the `create2` function of the ContractDeployer system contract,
    /// so that we could efficiently relay the calldata.
    function create2(
        bytes32, // _salt
        bytes32, // _bytecodeHash
        bytes calldata // _input
    ) external payable returns (address) {
        _relayCall();
    }

    /// @notice Function that calls the `create2Account` method of the `ContractDeployer` contract.
    /// @dev This function accepts the same parameters as the `create2Account` function of the ContractDeployer system contract,
    /// so that we could efficiently relay the calldata.
    function create2Account(
        bytes32, // _salt
        bytes32, // _bytecodeHash
        bytes calldata, // _input
        IContractDeployer.AccountAbstractionVersion // _aaVersion
    ) external payable returns (address) {
        _relayCall();
    }

    /// @notice Function that efficiently relays the calldata to the contract deployer system contract. After that,
    /// it also relays full result.
    function _relayCall() internal {
        bool success = EfficientCall.rawCall({
            _gas: gasleft(),
            _address: address(DEPLOYER_SYSTEM_CONTRACT),
            _value: msg.value,
            _data: msg.data,
            _isSystem: true
        });

        assembly {
            returndatacopy(0, 0, returndatasize())
            if success { return(0, returndatasize()) }
            revert(0, returndatasize())
        }
    }
}
