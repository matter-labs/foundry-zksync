// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "../cheats/Vm.sol";
import "./Bank.sol";
import "./Create2Utils.sol";
import "../default/logs/console.sol";

contract StorageAccessor {
    function read(bytes32 slot) public view returns (bytes32 value) {
        assembly {
            value := sload(slot)
        }
    }

    function write(bytes32 slot, bytes32 value) public {
        assembly {
            sstore(slot, value)
        }
    }
}

contract StorageAccessorDelegator {
    function accessDelegation(StorageAccessor store1, StorageAccessor store2) public {
        store1.read(bytes32(uint256(0x1b)));
        store1.write(bytes32(uint256(0x11)), bytes32(uint256(0x1a)));
        store2.write(bytes32(uint256(0x22)), bytes32(uint256(0x2a)));
        store2.write(bytes32(uint256(0x23)), bytes32(uint256(0x2b)));
    }
}

contract Payment {
    constructor() payable {}

    function pay() public payable {}
    function transact() public {}
}

contract PaymentDelegator {
    constructor() payable {}

    function payDelegation(Payment payee) public payable {
        payee.pay{value: msg.value}();
    }

    function transactDelegation(Payment payee) public {
        payee.transact();
    }
}

contract TransferDelegator {
    constructor() payable {}

    function transferDelegation1Eth(address payee) public payable returns (bool success) {
        (success,) = payable(payee).call{value: 1 ether}("");
    }
}

contract CreateDelegator {
    constructor() payable {}

    function createDelegation() public {
        address(new Bank());
        address(new Bank{value: 1 ether}());
    }
}

contract Create2Delegator {
    constructor() payable {}

    function create2Delegation() public {
        address(new Bank{salt: bytes32(uint256(0xf0))}());
        address(new Bank{value: 1 ether, salt: bytes32(uint256(0xf1))}());
    }
}

contract ZkStateDiffTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    StorageAccessor store1;
    StorageAccessor store2;
    StorageAccessorDelegator storeDelegator;
    Payment payment;
    PaymentDelegator paymentDelegator;
    TransferDelegator transferDelegator;
    CreateDelegator createDelegator;
    Create2Delegator create2Delegator;

    bytes32 bankBytecodeHash;

    function setUp() public {
        store1 = new StorageAccessor();
        store2 = new StorageAccessor();
        storeDelegator = new StorageAccessorDelegator();

        payment = new Payment();
        paymentDelegator = new PaymentDelegator();

        transferDelegator = new TransferDelegator{value: 5 ether}();
        createDelegator = new CreateDelegator{value: 5 ether}();
        create2Delegator = new Create2Delegator{value: 5 ether}();

        bankBytecodeHash = vm.parseJsonBytes32(vm.readFile("./zk/zkout/Bank.sol/Bank.json"), ".hash");
    }

    function testStateDiffReturnedForStorageAccesses() external {
        vm.startStateDiffRecording();

        store1.read(bytes32(uint256(0x1b)));
        store1.write(bytes32(uint256(0x11)), bytes32(uint256(0x1a)));
        store2.write(bytes32(uint256(0x22)), bytes32(uint256(0x2a)));
        store2.write(bytes32(uint256(0x23)), bytes32(uint256(0x2b)));

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());

        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](4);
        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0xB5c1DF089600415B21FB76bf89900Adb575947c8,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"61da1439000000000000000000000000000000000000000000000000000000000000001b",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: concat(
                Vm.StorageAccess({
                    account: 0xB5c1DF089600415B21FB76bf89900Adb575947c8,
                    slot: 0x000000000000000000000000000000000000000000000000000000000000001b,
                    isWrite: false,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    reverted: false
                })
            ),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[1] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0xB5c1DF089600415B21FB76bf89900Adb575947c8,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"e2e52ec10000000000000000000000000000000000000000000000000000000000000011000000000000000000000000000000000000000000000000000000000000001a",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: concat(
                Vm.StorageAccess({
                    account: 0xB5c1DF089600415B21FB76bf89900Adb575947c8,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000011,
                    isWrite: false,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    reverted: false
                }),
                Vm.StorageAccess({
                    account: 0xB5c1DF089600415B21FB76bf89900Adb575947c8,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000011,
                    isWrite: true,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x000000000000000000000000000000000000000000000000000000000000001a,
                    reverted: false
                })
            ),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[2] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"e2e52ec10000000000000000000000000000000000000000000000000000000000000022000000000000000000000000000000000000000000000000000000000000002a",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: concat(
                Vm.StorageAccess({
                    account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000022,
                    isWrite: false,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    reverted: false
                }),
                Vm.StorageAccess({
                    account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000022,
                    isWrite: true,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x000000000000000000000000000000000000000000000000000000000000002a,
                    reverted: false
                })
            ),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[3] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"e2e52ec10000000000000000000000000000000000000000000000000000000000000023000000000000000000000000000000000000000000000000000000000000002b",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: concat(
                Vm.StorageAccess({
                    account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000023,
                    isWrite: false,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    reverted: false
                }),
                Vm.StorageAccess({
                    account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000023,
                    isWrite: true,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x000000000000000000000000000000000000000000000000000000000000002b,
                    reverted: false
                })
            ),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForNestedStorageAccesses() external {
        vm.startStateDiffRecording();

        storeDelegator.accessDelegation(store1, store2);

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());

        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](5);
        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0x6914631e3e71Bc75A1664e3BaEE140CC05cAE18B,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"75d88b79000000000000000000000000b5c1df089600415b21fb76bf89900adb575947c8000000000000000000000000f9e9ba9ed9b96ab918c74b21dd0f1d5f2ac38a30",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[1] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Call,
            account: 0xB5c1DF089600415B21FB76bf89900Adb575947c8,
            accessor: 0x6914631e3e71Bc75A1664e3BaEE140CC05cAE18B,
            data: hex"61da1439000000000000000000000000000000000000000000000000000000000000001b",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: concat(
                Vm.StorageAccess({
                    account: 0xB5c1DF089600415B21FB76bf89900Adb575947c8,
                    slot: 0x000000000000000000000000000000000000000000000000000000000000001b,
                    isWrite: false,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    reverted: false
                })
            ),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[2] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Call,
            account: 0xB5c1DF089600415B21FB76bf89900Adb575947c8,
            accessor: 0x6914631e3e71Bc75A1664e3BaEE140CC05cAE18B,
            data: hex"e2e52ec10000000000000000000000000000000000000000000000000000000000000011000000000000000000000000000000000000000000000000000000000000001a",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: concat(
                Vm.StorageAccess({
                    account: 0xB5c1DF089600415B21FB76bf89900Adb575947c8,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000011,
                    isWrite: false,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    reverted: false
                }),
                Vm.StorageAccess({
                    account: 0xB5c1DF089600415B21FB76bf89900Adb575947c8,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000011,
                    isWrite: true,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x000000000000000000000000000000000000000000000000000000000000001a,
                    reverted: false
                })
            ),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[3] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Call,
            account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
            accessor: 0x6914631e3e71Bc75A1664e3BaEE140CC05cAE18B,
            data: hex"e2e52ec10000000000000000000000000000000000000000000000000000000000000022000000000000000000000000000000000000000000000000000000000000002a",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: concat(
                Vm.StorageAccess({
                    account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000022,
                    isWrite: false,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    reverted: false
                }),
                Vm.StorageAccess({
                    account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000022,
                    isWrite: true,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x000000000000000000000000000000000000000000000000000000000000002a,
                    reverted: false
                })
            ),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[4] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Call,
            account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
            accessor: 0x6914631e3e71Bc75A1664e3BaEE140CC05cAE18B,
            data: hex"e2e52ec10000000000000000000000000000000000000000000000000000000000000023000000000000000000000000000000000000000000000000000000000000002b",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: concat(
                Vm.StorageAccess({
                    account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000023,
                    isWrite: false,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    reverted: false
                }),
                Vm.StorageAccess({
                    account: 0xF9E9ba9Ed9B96AB918c74B21dD0f1D5f2ac38a30,
                    slot: 0x0000000000000000000000000000000000000000000000000000000000000023,
                    isWrite: true,
                    previousValue: 0x0000000000000000000000000000000000000000000000000000000000000000,
                    newValue: 0x000000000000000000000000000000000000000000000000000000000000002b,
                    reverted: false
                })
            ),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForCalls() external {
        vm.startStateDiffRecording();

        payment.transact();

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());
        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](1);

        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0x6b3D9bf4377eF0A0BE817B9e7B8D486AEE3b7876,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"af989083",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForNestedCalls() external {
        vm.startStateDiffRecording();

        paymentDelegator.transactDelegation(payment);

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());
        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](2);

        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0x2E7BD46C63696fb7B5136bd2b21B48821917ea7F,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"d1b910010000000000000000000000006b3d9bf4377ef0a0be817b9e7b8d486aee3b7876",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[1] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Call,
            account: 0x6b3D9bf4377eF0A0BE817B9e7B8D486AEE3b7876,
            accessor: 0x2E7BD46C63696fb7B5136bd2b21B48821917ea7F,
            data: hex"af989083",
            deployedCode: hex"",
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForCallWithValue() external {
        vm.startStateDiffRecording();

        payment.pay{value: 1 ether}();

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());
        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](1);

        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0x6b3D9bf4377eF0A0BE817B9e7B8D486AEE3b7876,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"1b9265b8",
            deployedCode: hex"",
            value: 1000000000000000000,
            oldBalance: 0,
            newBalance: 1000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForNestedCallWithValue() external {
        vm.startStateDiffRecording();

        paymentDelegator.payDelegation{value: 1 ether}(payment);

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());
        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](2);

        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0x2E7BD46C63696fb7B5136bd2b21B48821917ea7F,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"92cdd1870000000000000000000000006b3d9bf4377ef0a0be817b9e7b8d486aee3b7876",
            deployedCode: hex"",
            value: 1000000000000000000,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[1] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Call,
            account: 0x6b3D9bf4377eF0A0BE817B9e7B8D486AEE3b7876,
            accessor: 0x2E7BD46C63696fb7B5136bd2b21B48821917ea7F,
            data: hex"1b9265b8",
            deployedCode: hex"",
            value: 1000000000000000000,
            oldBalance: 0,
            newBalance: 1000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForTransfer() external {
        vm.startStateDiffRecording();

        // fails for 65536 and lower. This seems to fail now below 65544.
        (bool success,) = payable(address(65557)).call{value: 1 ether}("");
        require(success);

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());
        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](1);

        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: address(65557),
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"",
            deployedCode: hex"",
            value: 1000000000000000000,
            oldBalance: 0,
            newBalance: 1000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForNestedTransfer() external {
        vm.startStateDiffRecording();

        // fails for 65536 and lower. This seems to fail now below 65544.
        bool success = transferDelegator.transferDelegation1Eth(address(65557));
        require(success);

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());
        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](2);

        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0x24b38E0835586dFd1716Cd263F9890ba0306dCa8,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"7a8e32ca0000000000000000000000000000000000000000000000000000000000010015",
            deployedCode: hex"",
            value: 0,
            oldBalance: 5000000000000000000,
            newBalance: 4000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[1] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Call,
            account: address(65557),
            accessor: 0x24b38E0835586dFd1716Cd263F9890ba0306dCa8,
            data: hex"",
            deployedCode: hex"",
            value: 1000000000000000000,
            oldBalance: 0,
            newBalance: 1000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForCreate() external {
        vm.startStateDiffRecording();

        address(new Bank());
        address(new Bank{value: 1 ether}());

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());
        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](2);

        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Create,
            account: 0x12db303A83e945CDBeB72359Ec365D2bd63d331E,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"",
            deployedCode: abi.encodePacked(bankBytecodeHash),
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[1] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Create,
            account: 0xf22ee22d4241fB723420Bec92D59f9913F1C949f,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"",
            deployedCode: abi.encodePacked(bankBytecodeHash),
            value: 1000000000000000000,
            oldBalance: 0,
            newBalance: 1000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForNestedCreate() external {
        vm.startStateDiffRecording();

        createDelegator.createDelegation();

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());
        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](3);

        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0xa4e69fB667e67734817b27C4b44a3b03542912D6,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"6d1ed806",
            deployedCode: hex"",
            value: 0,
            oldBalance: 5000000000000000000,
            newBalance: 4000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[1] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Create,
            account: 0x1F586b3A8E212336d1e3876e738314907732b7D5,
            accessor: 0xa4e69fB667e67734817b27C4b44a3b03542912D6,
            data: hex"",
            deployedCode: abi.encodePacked(bankBytecodeHash),
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[2] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Create,
            account: 0xBC29fab1B038dBcfAE7099FE15e037584df360a2,
            accessor: 0xa4e69fB667e67734817b27C4b44a3b03542912D6,
            data: hex"",
            deployedCode: abi.encodePacked(bankBytecodeHash),
            value: 1000000000000000000,
            oldBalance: 0,
            newBalance: 1000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForCreate2() external {
        vm.startStateDiffRecording();

        bytes32 salt1 = bytes32(uint256(0xe0));
        bytes32 salt2 = bytes32(uint256(0xe1));
        address bankAddr1 =
            Create2Utils.computeCreate2Address(address(this), salt1, bankBytecodeHash, keccak256(abi.encode()));
        address bankAddr2 =
            Create2Utils.computeCreate2Address(address(this), salt2, bankBytecodeHash, keccak256(abi.encode()));

        address(new Bank{salt: salt1}());
        address(new Bank{value: 1 ether, salt: salt2}());

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());
        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](2);

        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Create,
            account: bankAddr1,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"",
            deployedCode: abi.encodePacked(bankBytecodeHash),
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[1] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Create,
            account: bankAddr2,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"",
            deployedCode: abi.encodePacked(bankBytecodeHash),
            value: 1000000000000000000,
            oldBalance: 0,
            newBalance: 1000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function testStateDiffReturnedForNestedCreate2() external {
        vm.startStateDiffRecording();

        address bankAddr1 = Create2Utils.computeCreate2Address(
            address(create2Delegator), bytes32(uint256(0xf0)), bankBytecodeHash, keccak256(abi.encode())
        );
        address bankAddr2 = Create2Utils.computeCreate2Address(
            address(create2Delegator), bytes32(uint256(0xf1)), bankBytecodeHash, keccak256(abi.encode())
        );

        create2Delegator.create2Delegation();

        Vm.AccountAccess[] memory diff = filterCallOrCreate(vm.stopAndReturnStateDiff());
        Vm.ChainInfo memory chainInfo = Vm.ChainInfo(0, 31337);
        Vm.AccountAccess[] memory expected = new Vm.AccountAccess[](3);

        expected[0] = Vm.AccountAccess({
            depth: 1,
            kind: Vm.AccountAccessKind.Call,
            account: 0x38C6337a87f3479f8E55789da8B9334Da21416FC,
            accessor: 0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496,
            data: hex"3893da7f",
            deployedCode: hex"",
            value: 0,
            oldBalance: 5000000000000000000,
            newBalance: 4000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[1] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Create,
            account: bankAddr1,
            accessor: 0x38C6337a87f3479f8E55789da8B9334Da21416FC,
            data: hex"",
            deployedCode: abi.encodePacked(bankBytecodeHash),
            value: 0,
            oldBalance: 0,
            newBalance: 0,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });
        expected[2] = Vm.AccountAccess({
            depth: 2,
            kind: Vm.AccountAccessKind.Create,
            account: bankAddr2,
            accessor: 0x38C6337a87f3479f8E55789da8B9334Da21416FC,
            data: hex"",
            deployedCode: abi.encodePacked(bankBytecodeHash),
            value: 1000000000000000000,
            oldBalance: 0,
            newBalance: 1000000000000000000,
            storageAccesses: new Vm.StorageAccess[](0),
            chainInfo: chainInfo,
            initialized: true,
            reverted: false
        });

        assertEq(expected, diff);
    }

    function concat(Vm.StorageAccess memory a) internal pure returns (Vm.StorageAccess[] memory out) {
        out = new Vm.StorageAccess[](1);
        out[0] = a;
    }

    function concat(Vm.StorageAccess memory a, Vm.StorageAccess memory b)
        internal
        pure
        returns (Vm.StorageAccess[] memory out)
    {
        out = new Vm.StorageAccess[](2);
        out[0] = a;
        out[1] = b;
    }

    function filterCallOrCreate(Vm.AccountAccess[] memory inArr)
        internal
        pure
        returns (Vm.AccountAccess[] memory out)
    {
        // allocate max length for out array
        out = new Vm.AccountAccess[](inArr.length);
        // track end size
        uint256 size;
        for (uint256 i = 0; i < inArr.length; ++i) {
            if (
                inArr[i].kind == Vm.AccountAccessKind.Call || inArr[i].kind == Vm.AccountAccessKind.StaticCall
                    || inArr[i].kind == Vm.AccountAccessKind.Create
            ) {
                out[size] = inArr[i];
                ++size;
            }
        }
        // manually truncate out array
        assembly {
            mstore(out, size)
        }
    }

    function assertEq(Vm.AccountAccess[] memory want, Vm.AccountAccess[] memory got) internal {
        assertEq(want.length, got.length, "account accesses length mismatch");
        for (uint256 i = 0; i < want.length; ++i) {
            assertEq(want[i].depth, got[i].depth, keyField(i, "depth"));
            assertEq(uint8(want[i].kind), uint8(got[i].kind), keyField(i, "kind"));
            assertEq(want[i].account, got[i].account, keyField(i, "account"));
            assertEq(want[i].accessor, got[i].accessor, keyField(i, "accessor"));
            assertEq(want[i].data, got[i].data, keyField(i, "data"));
            assertEq(want[i].deployedCode, got[i].deployedCode, keyField(i, "deployedCode"));
            assertEq(want[i].value, got[i].value, keyField(i, "value"));
            assertEq(want[i].oldBalance, got[i].oldBalance, keyField(i, "oldBalance"));
            assertEq(want[i].newBalance, got[i].newBalance, keyField(i, "newBalance"));

            assertEq(want[i].storageAccesses.length, got[i].storageAccesses.length, "storage accesses length mismatch");
            for (uint256 j = 0; j < want[i].storageAccesses.length; ++j) {
                assertEq(
                    want[i].storageAccesses[j].account, got[i].storageAccesses[j].account, keyStorage(i, j, "account")
                );
                assertEq(want[i].storageAccesses[j].slot, got[i].storageAccesses[j].slot, keyStorage(i, j, "slot"));
                assertEq(
                    vm.toString(want[i].storageAccesses[j].isWrite),
                    vm.toString(got[i].storageAccesses[j].isWrite),
                    keyStorage(i, j, "isWrite")
                );
                assertEq(
                    want[i].storageAccesses[j].previousValue,
                    got[i].storageAccesses[j].previousValue,
                    keyStorage(i, j, "previousValue")
                );
                assertEq(
                    want[i].storageAccesses[j].newValue,
                    got[i].storageAccesses[j].newValue,
                    keyStorage(i, j, "newValue")
                );
            }
        }
    }

    function keyField(uint256 index, string memory field) internal pure returns (string memory) {
        return string.concat("[", vm.toString(index), "].", field);
    }

    function keyStorage(uint256 index, uint256 storageIndex, string memory field)
        internal
        pure
        returns (string memory)
    {
        return string.concat("[", vm.toString(index), "].storage[", vm.toString(storageIndex), "].", field);
    }
}
