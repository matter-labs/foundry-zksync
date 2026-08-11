// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.7;

import "utils/Test.sol";

import "./LibrariesFactoryDeps.sol";

contract DeployTimeLinkingFactoryDeps is Test {
    function test_libraryIsPersisted() external {
        console.log(address(LibraryWithDep));  // use library

        LibraryDep dep = new LibraryDep();  // use library dep
        assertEq(1, dep.one());
    }
}
