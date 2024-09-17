// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.7 <0.9.0;

import {Globals} from "./Globals.sol";

// excerpt from system-contracts
interface ISystemContext {
    function chainId() external view returns (uint256);
}

library InZkVmLib {
    function _inZkVm() internal returns (bool) {
        (bool success, bytes memory retdata) =
            Globals.SYSTEM_CONTEXT_ADDR.call(abi.encodeWithSelector(ISystemContext.chainId.selector));

        return success;
    }
}

abstract contract InZkVm {
    modifier inZkVm() {
        require(InZkVmLib._inZkVm(), "must be executed in zkVM");
        _;
    }
}

abstract contract DeployOnlyInZkVm is InZkVm {
    constructor() {
        require(InZkVmLib._inZkVm(), "must be deployed in zkVM");
    }
}
