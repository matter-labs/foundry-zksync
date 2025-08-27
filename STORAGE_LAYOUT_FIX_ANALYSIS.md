# Storage Layout Fix Analysis

## Problem Summary

During an upstream merge, foundry-zksync encountered a critical issue where storage layout tests (`test_state_diff_storage_layout`) were failing because state diff JSON showed `"label":null` instead of expected storage labels like `"matrix[0][0]"`.

## Root Cause Analysis

### The Issue
The problem stemmed from foundry-zksync's **strategy pattern architecture** creating a data flow separation that doesn't exist in upstream foundry:

- **Gas reporting tests** need **linked bytecode** from contracts
- **Storage layout tests** need **storage layouts** from original compile output  
- **`ContractsByArtifact::new()`** only preserved linked bytecode (no storage layouts)
- **`ContractsByArtifact::with_storage_layout()`** only preserved storage layouts (unlinked bytecode)

### Technical Root Cause
The zkSync strategy was using `ContractsByArtifact::new(linked_contracts.clone())` which hardcodes `storage_layout: None`, while the EVM strategy correctly preserved storage layouts but the zkSync strategy couldn't access them.

## Architecture Comparison

### Upstream Foundry (Working)
```rust
// MultiContractRunnerBuilder::build() - SINGLE CONTEXT
pub fn build(
    self,
    root: &Path,
    output: &ProjectCompileOutput,  // ← Available here
    env: Env,
    evm_opts: EvmOpts,
) -> Result<MultiContractRunner> {
    // Step 1: Create linker and link contracts
    let linker = Linker::new(root, contracts);
    let LinkOutput { libraries, libs_to_deploy } = linker.link_with_nonce_or_address(..)?;
    let linked_contracts = linker.get_linked_artifacts(&libraries)?;  // ← Available here
    
    // Step 2: BOTH pieces available simultaneously - can combine!
    let known_contracts = ContractsByArtifactBuilder::new(linked_contracts)  // ← Linked bytecode
        .with_storage_layouts(output.clone())  // ← Storage layouts from original
        .build();
        
    Ok(MultiContractRunner { known_contracts, .. })
}
```

### foundry-zksync (Broken Data Pipeline)
```rust
// MultiContractRunnerBuilder::build() - CONTEXT 1
pub fn build(..) -> Result<MultiContractRunner> {
    let LinkOutput { known_contracts, .. } = strategy.runner.link(..)?;  // ← Data loss happens HERE
    Ok(MultiContractRunner { known_contracts, .. })
}

// EvmExecutorStrategyRunner::link() - CONTEXT 2  
fn link(&self, input: &ProjectCompileOutput, ..) -> Result<LinkOutput> {
    let linked_contracts = linker.get_linked_artifacts(&libraries)?;
    
    // FORCED CHOICE: Can only return ONE ContractsByArtifact, not both pieces
    let known_contracts = ContractsByArtifact::new(linked_contracts);  // ← Loses storage layouts
    // OR
    let known_contracts = ContractsByArtifact::with_storage_layout(input);  // ← Loses linked bytecode
    
    Ok(LinkOutput { known_contracts, .. })  // ← Information loss encoded in return type
}

// ZkSyncExecutorStrategyRunner::link() - CONTEXT 3
fn link(&self, ..) -> Result<LinkOutput> {
    let evm_link = EvmExecutorStrategyRunner.link(..)?;  // ← Gets pre-decided choice
    // Too late - can't access original `input` with storage layouts AND linked contracts
    Ok(LinkOutput { known_contracts: evm_link.known_contracts, .. })
}
```

### Core Problem: Information Architecture Mismatch

1. **Context Fragmentation**: Upstream has a single context where `linked_contracts` and `original_output` coexist. The strategy pattern breaks this into multiple isolated contexts.

2. **Return Type Constraint**: `LinkOutput` can only contain a single `ContractsByArtifact`, forcing a binary choice between linked bytecode OR storage layouts.

3. **Sequential Data Loss**: Each strategy can only see the output of the previous strategy, not the original inputs.

## Solution Implemented: Hybrid Constructor Approach

### What Was Done
Created a new constructor `ContractsByArtifact::with_linked_bytecode_and_storage_layout()` that combines both:
- Takes linked contracts (for gas reporting functionality)
- Takes original compile output (for storage layout information)  
- Merges them to create contracts with **both** linked bytecode AND storage layouts

### Implementation Details
```rust
// Added to crates/common/src/contracts.rs
/// Creates a new instance by combining linked contracts with storage layouts from original output.
/// This provides both linked bytecode (for gas reporting) and storage layouts (for state diff).
pub fn with_linked_bytecode_and_storage_layout(
    linked_contracts: impl IntoIterator<Item = (ArtifactId, CompactContractBytecode)>,
    original_output: ProjectCompileOutput,
) -> Self {
    let storage_layouts: BTreeMap<ArtifactId, _> = original_output
        .into_artifacts()
        .filter_map(|(id, artifact)| {
            artifact.storage_layout.map(|layout| (id, Arc::new(layout)))
        })
        .collect();

    let map = linked_contracts
        .into_iter()
        .filter_map(|(id, artifact)| {
            let name = id.name.clone();
            let CompactContractBytecode { abi, bytecode, deployed_bytecode } = artifact;
            let storage_layout = storage_layouts.get(&id).cloned();
            Some((
                id,
                ContractData {
                    name,
                    abi: abi?,
                    bytecode: bytecode.map(Into::into),
                    deployed_bytecode: deployed_bytecode.map(Into::into),
                    storage_layout,
                },
            ))
        })
        .collect();
    Self(Arc::new(map))
}

// Updated crates/evm/evm/src/executors/strategy/libraries.rs
let known_contracts = ContractsByArtifact::with_linked_bytecode_and_storage_layout(
    linked_contracts.clone(),
    input.clone(),
);
```

### Why This Fix Works
- **Preserves gas reporting**: Uses linked bytecode from the linker
- **Preserves storage layouts**: Overlays storage layout information from original compilation  
- **Doesn't break existing tests**: Unlike previous attempts that broke multiple gas reporting tests
- **Minimal breaking changes**: Existing code continues working

## Alternative Architectural Solutions

### Option 1: Deferred Combination Architecture (Recommended Long-term)
Move the combination logic up to `MultiContractRunnerBuilder` like upstream:

```rust
// Modified LinkOutput - return RAW MATERIALS instead of final result
pub struct LinkOutput {
    pub deployable_contracts: BTreeMap<ArtifactId, (JsonAbi, Bytes)>,
    pub revert_decoder: RevertDecoder,
    pub linked_contracts: ArtifactContracts,  // ← Keep this
    // REMOVE: known_contracts: ContractsByArtifact,  ← Don't pre-combine here
    pub libs_to_deploy: Vec<Bytes>,
    pub libraries: Libraries,
}

// MultiContractRunnerBuilder::build() - like upstream
pub fn build(..) -> Result<MultiContractRunner> {
    let LinkOutput { linked_contracts, .. } = strategy.runner.link(..)?;
    
    // DEFER combination to here where we have access to original `output`
    let known_contracts = ContractsByArtifactBuilder::new(linked_contracts)
        .with_storage_layouts(output.clone())
        .build();
        
    Ok(MultiContractRunner { known_contracts, .. })
}
```

**Benefits:**
- Exact same logic as upstream foundry
- No custom hybrid methods needed
- Clean separation of concerns
- Future-proof against upstream changes

**Challenges:**
- Breaking change to `LinkOutput` structure
- All strategy implementations need updates
- `ContractsByArtifactBuilder` doesn't exist in foundry-zksync yet

### Option 2: Rich Context Architecture  
Pass storage layout information through the strategy chain:

```rust
// Enhanced strategy context
pub struct EnhancedExecutorStrategyContext {
    pub original_output: ProjectCompileOutput,  // ← Keep original available
    // ... other context
}

// Strategies can now access original output
impl ZkSyncExecutorStrategyRunner {
    fn link(&self, ctx: &EnhancedExecutorStrategyContext, ..) -> Result<LinkOutput> {
        let evm_link = EvmExecutorStrategyRunner.link(ctx, ..)?;
        
        // Can access original output here!
        let known_contracts = ContractsByArtifactBuilder::new(evm_link.linked_contracts)
            .with_storage_layouts(ctx.original_output.clone())
            .build();
            
        Ok(LinkOutput { known_contracts, .. })
    }
}
```

### Option 3: Strategy Interface Evolution
Change the strategy interface to be more like upstream's builder:

```rust
// New strategy interface
pub trait ExecutorStrategyRunner {
    fn link(&self, ..) -> Result<RawLinkOutput>;
    
    // NEW: Let strategy participate in final combination
    fn build_contracts(
        &self, 
        raw_output: RawLinkOutput,
        original_output: ProjectCompileOutput
    ) -> ContractsByArtifact;
}
```

## Trade-off Analysis & Recommendations

### Current Solution (Hybrid) - Best Short-term
- **Implementation Effort**: ⭐⭐ (Low - already done)
- **Breaking Changes**: ⭐ (Minimal)
- **Upstream Alignment**: ⭐⭐ (Divergent)
- **Maintainability**: ⭐⭐⭐ (Good but custom)

### Option 1 (Deferred Combination) - Best Long-term
- **Implementation Effort**: ⭐⭐⭐⭐ (High)
- **Breaking Changes**: ⭐⭐⭐⭐⭐ (Significant)  
- **Upstream Alignment**: ⭐⭐⭐⭐⭐ (Perfect)
- **Maintainability**: ⭐⭐⭐⭐⭐ (Excellent)

## Strategic Recommendation

### Phase 1 (Current): Keep the hybrid approach for stability
- ✅ Fixes the immediate problem
- ✅ Allows testing and validation  
- ✅ No disruption to existing code

### Phase 2 (Future): Migrate to Option 1 (Deferred Combination)
- 🎯 **Create ContractsByArtifactBuilder** (port from upstream)
- 🎯 **Refactor LinkOutput** to return raw materials
- 🎯 **Update MultiContractRunnerBuilder** to match upstream pattern
- 🎯 **Benefits**: Perfect upstream alignment, future-proof architecture

This approach provides the best of both worlds: immediate fix with hybrid approach + long-term architectural alignment with upstream foundry.

## Why foundry-zksync Needs This While Upstream Doesn't

The fundamental issue is that **foundry-zksync's strategy pattern creates architectural constraints that don't exist in upstream foundry**:

- **Upstream**: Single context in `MultiContractRunnerBuilder::build()` with access to both linked contracts and original output
- **foundry-zksync**: Multi-stage strategy pattern that fragments context and forces binary choices in `LinkOutput`

The strategy pattern provides benefits for zkSync-specific functionality but creates this data flow separation that upstream doesn't have, requiring the hybrid solution to bridge the gap.

## Files Modified
- `crates/common/src/contracts.rs`: Added `with_linked_bytecode_and_storage_layout()` method
- `crates/evm/evm/src/executors/strategy/libraries.rs`: Updated to use new hybrid constructor

## Commit Reference
- Fix commit: `0ff7a41dae` - "Add hybrid ContractsByArtifact constructor"
- Removed problematic commit: `ef9573511f` - "fix remaining tests" (broke gas reporting tests)