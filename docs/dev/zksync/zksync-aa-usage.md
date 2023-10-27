## Guide to Deploying and Interacting with Account Abstraction Multisig Contracts on zkSync Era

In this guide, we'll go through the process of compiling, deploying, and interacting with contracts for account abstraction multisig on the zkSync Era platform. We'll work with two contracts: `AAFactory.sol` and `TwoUserMultiSig.sol`.

### Step 1: Compile `AAFactory.sol`

First, compile `AAFactory.sol` using the `--is-system` flag because it interacts with system contracts for deploying multisig wallets.

**Location:** Place the contract in the `src/is-system/` folder.

**Command:**
```sh
../foundry-zksync/target/debug/zkforge zk-build
```

**Expected Output:**
```sh
AAFactory -> Bytecode Hash: "010000791703a54dbe2502b00ee470989c267d0f6c0d12a9009a947715683744" 
Compiled Successfully
```

### Step 2: Deploy `AAFactory.sol`

To deploy the factory, use the Bytecode Hash of `TwoUserMultiSig.sol` in the constructor of `AAFactory.sol`.

**Note:** `aaBytecodeHash` equals the Bytecode hash of `TwoUserMultiSig.sol`.

**Command:**
```sh
../foundry-zksync/target/debug/zkforge zkc src/is-system/AAFactory.sol:AAFactory --constructor-args 010007572230f4df5b4e855ff48d4cdfffc9405522117d7e020ee42650223460 --factory-deps src/TwoUserMultiSig.sol:TwoUserMultisig --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --rpc-url http://localhost:3050 --chain 270
```

**Expected Output:**
```sh
Deploying contract...
Contract successfully deployed to address: 0xd5608cec132ed4875d19f8d815ec2ac58498b4e5
Transaction Hash: 0x0e6f55ff1619af8b3277853a8f2941d0481635880358316f03ae264e2de059ed
Gas used: 154379
Effective gas price: 250000000
Block Number: 291
```

### Step 3: Deploy `TwoUserMultiSig.sol` Instance

Now, deploy a new `TwoUserMultiSig.sol` instance using the `deployAccount` function of `AAFactory.sol`.

**Required Parameters:**
- **owner1:** `0xa61464658AfeAf65CccaaFD3a512b69A83B77618`
- **owner2:** `0x0D43eB5B8a47bA8900d84AA36656c92024e9772e`
- **salt:** `0x00` (unique value needed for each instance using the same owner wallets).

**Command:**
```sh
../foundry-zksync/target/debug/zkcast zk-send 0xd5608cec132ed4875d19f8d815ec2ac58498b4e5 "deployAccount(bytes32,address,address)(address)" 0x00 0xa61464658AfeAf65CccaaFD3a512b69A83B77618 0x0D43eB5B8a47bA8900d84AA36656c92024e9772e --rpc-url http://localhost:3050 --private-key 7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --chain 270
```

**Expected Output:**
```sh
Sending transaction....
Transaction Hash: 0x43a4dded84a12891dfae4124b42b9f091750e953193bd779a7e5e4d422909e73
0x03e50ec034f1d363de0add752c33d4831a2731bf, <---- Deployed contract address
```

### Step 4: Verify the Deployment

Check the transaction receipt and verify the owners of the deployed `TwoUserMultiSig.sol` contract.

**Command to Check Transaction Receipt:**
```sh
../foundry-zksync/target/debug/zkcast tx 0x22364a3e191ad10013c5f20036e9696e743a4f686bc58a0106ef0b9e7592347c --rpc-url http://localhost:3050
```

**Verify `owner1`:**
```sh
../foundry-zksync/target/debug/zkcast call 0x03e50ec034f1d363de0add752c33d4831a2731bf "owner1()(address)" --rpc-url http://localhost:3050
```

**Expected Output for `owner1`:**
```txt
0xa61464658AfeAf65CccaaFD3a512b69A83B77618
```

**Verify `owner2`:**
```sh
../foundry-zksync/target/debug/zkcast call 0x03e50ec034f1d363de0add752c33d4831a2731bf "owner2()(address)" --rpc-url http://localhost:3050
```

**Expected Output for `owner2`:**
```txt
0x0D43eB5B8a47bA8900d84AA36656c92024e9772e
```

With these steps completed, you should have successfully deployed and verified a `TwoUserMultiSig.sol` contract instance on zkSync Era.