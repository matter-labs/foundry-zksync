// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script} from "forge-std/Script.sol";
import {Greeter} from "../src/Greeter.sol";

interface VmExt {
    function zkGetTransactionNonce(
        address account
    ) external view returns (uint64 nonce);
    function zkGetDeploymentNonce(
        address account
    ) external view returns (uint64 nonce);
}

contract ScriptSetupNonce is Script {
    VmExt internal constant vmExt = VmExt(VM_ADDRESS);

    function setUp() public {
        uint256 initialNonceTx = checkTxNonce(address(tx.origin));
        uint256 initialNonceDeploy = checkDeployNonce(address(tx.origin));
        // Perform transactions and deploy contracts in setup to increment nonce and verify broadcast nonce matches onchain
        new Greeter();
        new Greeter();
        new Greeter();
        new Greeter();
        assert(checkTxNonce(address(tx.origin)) == initialNonceTx);
        assert(checkDeployNonce(address(tx.origin)) == initialNonceDeploy);
    }

    function run() public {
        // Get initial nonce
        uint256 initialNonceTx = checkTxNonce(tx.origin);
        uint256 initialNonceDeploy = checkDeployNonce(tx.origin);
        assert(initialNonceTx == vmExt.zkGetTransactionNonce(tx.origin));
        assert(initialNonceDeploy == vmExt.zkGetDeploymentNonce(tx.origin));

        // Create and interact with non-broadcasted contract to verify nonce is not incremented
        Greeter notBroadcastGreeter = new Greeter();
        notBroadcastGreeter.greeting("john");
        assert(checkTxNonce(tx.origin) == initialNonceTx);
        assert(checkDeployNonce(tx.origin) == initialNonceDeploy);

        // Start broadcasting transactions
        vm.startBroadcast();
        // Deploy and interact with broadcasted contracts
        Greeter greeter = new Greeter();
        greeter.greeting("john");

        // Deploy checker and verify nonce
        NonceChecker checker = new NonceChecker();

        vm.stopBroadcast();

        // We expect the nonce to be incremented by 1 because the check is done in an external
        // call
        checker.assertTxNonce(
            vmExt.zkGetTransactionNonce(address(tx.origin)) + 1
        );
        checker.assertDeployNonce(
            vmExt.zkGetDeploymentNonce(address(tx.origin))
        );
    }

    function checkTxNonce(address addr) public returns (uint256) {
        // We prank here to avoid accidentally "polluting" the nonce of `addr` during the call
        // for example when `addr` is `tx.origin`
        vm.prank(address(this), address(this));
        return NonceLib.getTxNonce(addr);
    }

    function checkDeployNonce(address addr) public returns (uint256) {
        // We prank here to avoid accidentally "polluting" the nonce of `addr` during the call
        // for example when `addr` is `tx.origin`
        vm.prank(address(this), address(this));
        return NonceLib.getDeployNonce(addr);
    }
}

contract NonceChecker {
    function checkTxNonce() public returns (uint256) {
        return NonceLib.getTxNonce(address(tx.origin));
    }

    function checkDeployNonce() public returns (uint256) {
        return NonceLib.getDeployNonce(address(tx.origin));
    }

    function assertTxNonce(uint256 expected) public {
        uint256 real_nonce = checkTxNonce();
        require(real_nonce == expected, "tx nonce mismatch");
    }

    function assertDeployNonce(uint256 expected) public {
        uint256 real_nonce = checkDeployNonce();
        require(real_nonce == expected, "deploy nonce mismatch");
    }
}

library NonceLib {
    address constant NONCE_HOLDER = address(0x8003);

    /// Retrieve tx nonce for `addr` from the NONCE_HOLDER system contract
    function getTxNonce(address addr) internal returns (uint256) {
        (bool success, bytes memory data) = NONCE_HOLDER.call(
            abi.encodeWithSignature("getMinNonce(address)", addr)
        );
        require(success, "Failed to get nonce");
        return abi.decode(data, (uint256));
    }

    /// Retrieve tx nonce for `addr` from the NONCE_HOLDER system contract
    function getDeployNonce(address addr) internal returns (uint256) {
        (bool success, bytes memory data) = NONCE_HOLDER.call(
            abi.encodeWithSignature("getDeploymentNonce(address)", addr)
        );
        require(success, "Failed to get nonce");
        return abi.decode(data, (uint256));
    }
}
