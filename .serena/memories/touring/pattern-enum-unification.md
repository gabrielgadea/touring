# Pattern: Enum Unification for Divergent Return Types

## Use Case
When two code paths return different types but need to be unified at a consumer boundary.

## Solution: Type-Normalized Enum

```rust
pub enum BlastRadiusOutput {
    Files(Vec<PathBuf>),
    Rich(BlastRadius),
}

impl BlastRadiusOutput {
    /// Extract Vec<PathBuf> regardless of variant
    pub fn files(&self) -> Vec<PathBuf> {
        match self {
            Self::Files(paths) => paths.clone(),
            Self::Rich(blast) => blast.files(),
        }
    }
}

impl From<Vec<PathBuf>> for BlastRadiusOutput {
    fn from(files: Vec<PathBuf>) -> Self {
        Self::Files(files)
    }
}

impl From<BlastRadius> for BlastRadiusOutput {
    fn from(rich: BlastRadius) -> Self {
        Self::Rich(rich)
    }
}
```

## Key Insight
- `.files()` method abstracts variant extraction
- `From` impls enable ergonomic `into()` conversions
- Consumer code only sees unified enum, not internal type divergence
