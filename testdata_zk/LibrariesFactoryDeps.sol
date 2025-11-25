// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.7 <0.9.0;

contract LibraryDep {
    function one() public pure returns (uint256) {
        return 1;
    }
}

library LibraryWithDep {
    // currently calling this function will not work but we use it to add a dependency
    function multiply(uint256 a, uint256 b) public returns (uint256) {
        LibraryDep dep = new LibraryDep();
        return a * b * dep.one();
    }
}
