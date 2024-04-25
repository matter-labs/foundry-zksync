// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";

contract FixedSlot {
    uint8 num; // slot index: 0

    function setSlot0(uint8 _num) public {
        num = _num;
    }
}

contract InnerMock {
    function getBytes() public payable returns (bytes memory) {
        bytes memory r = bytes(hex"abcd");
        return r;
    }
}

contract Mock {
    InnerMock private inner;

    constructor(InnerMock _inner) payable {
        inner = _inner;
    }

    function getBytes() public returns (bytes memory) {
        return inner.getBytes{value: 10}();
    }
}

interface IMyProxyCaller {
    function transact(uint8 _data) external;
}

contract MyProxyCaller {
    function transact(address inner) public {
        IMyProxyCaller(inner).transact(10);
    }
}

contract ZkCheatcodesTest is Test {
    uint256 testSlot = 0; //0x000000000000000000000000000000000000000000000000000000000000001e slot
    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    uint256 constant ETH_FORK_BLOCK = 19225195;
    uint256 constant ETH_FORK_BLOCK_TS = 1707901427;

    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;

    uint256 forkEra;
    uint256 forkEth;

    function setUp() public {
        forkEra = vm.createFork("mainnet", ERA_FORK_BLOCK);
        forkEth = vm.createFork(
            "https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf",
            ETH_FORK_BLOCK
        );
    }

    function testZkCheatcodesRoll() public {
        vm.selectFork(forkEra);
        require(block.number == ERA_FORK_BLOCK, "era block number mismatch");

        vm.roll(ERA_FORK_BLOCK + 1);
        require(
            block.number == ERA_FORK_BLOCK + 1,
            "era block number mismatch"
        );
    }

    function testZkCheatcodesWarp() public {
        vm.selectFork(forkEra);
        require(
            block.timestamp == ERA_FORK_BLOCK_TS,
            "era block timestamp mismatch"
        );

        vm.warp(ERA_FORK_BLOCK_TS + 1);
        require(
            block.timestamp == ERA_FORK_BLOCK_TS + 1,
            "era block timestamp mismatch"
        );
    }

    function testZkCheatcodesDeal() public {
        vm.selectFork(forkEra);
        require(TEST_ADDRESS.balance == 0, "era balance mismatch");

        vm.deal(TEST_ADDRESS, 100);
        require(TEST_ADDRESS.balance == 100, "era balance mismatch");
    }

    function testZkCheatcodesSetNonce() public {
        vm.selectFork(forkEra);
        require(vm.getNonce(TEST_ADDRESS) == 0, "era nonce mismatch");

        vm.setNonce(TEST_ADDRESS, 10);
        require(vm.getNonce(TEST_ADDRESS) == 10, "era nonce mismatch");

        vm.resetNonce(TEST_ADDRESS);
        require(vm.getNonce(TEST_ADDRESS) == 0, "era nonce mismatch");
    }

    function testZkCheatcodesEtch() public {
        vm.selectFork(forkEra);

        string memory artifact = vm.readFile(
            "zkout/ConstantNumber.sol/artifacts.json"
        );
        bytes memory constantNumberCode = vm.parseJsonBytes(
            artifact,
            '.contracts.["src/ConstantNumber.sol"].ConstantNumber.evm.bytecode.object'
        );
        vm.etch(TEST_ADDRESS, constantNumberCode);

        (bool success, bytes memory output) = TEST_ADDRESS.call(
            abi.encodeWithSignature("ten()")
        );
        require(success, "ten() call failed");

        uint8 number = abi.decode(output, (uint8));
        require(number == 10, "era etched code incorrect");
    }

    function testRecord() public {
        FixedSlot fs = new FixedSlot();
        vm.record();
        fs.setSlot0(10);
        (bytes32[] memory reads, bytes32[] memory writes) = vm.accesses(
            address(fs)
        );
        bytes32 keySlot0 = bytes32(uint256(0));
        assertEq(reads[0], keySlot0);
        assertEq(writes[0], keySlot0);
    }
    function testZkCheatcodesValueFunctionMockReturn() public {
        InnerMock inner = new InnerMock();
        // Send some funds to so it can pay for the inner call
        Mock target = new Mock{value: 50}(inner);

        bytes memory dataBefore = target.getBytes();
        assertEq(dataBefore, bytes(hex"abcd"));

        vm.mockCall(
            address(inner),
            abi.encodeWithSelector(inner.getBytes.selector),
            abi.encode(bytes(hex"a1b1"))
        );

        bytes memory dataAfter = target.getBytes();
        assertEq(dataAfter, bytes(hex"a1b1"));
    }

     function testZkCheatcodesCanMockCallTestContract() public {
        address thisAddress = address(this);

        vm.mockCall(
            thisAddress,
            abi.encodeWithSelector(IMyProxyCaller.transact.selector),
            abi.encode()
        );

        MyProxyCaller transactor = new MyProxyCaller();
        transactor.transact(thisAddress);
    }
}
