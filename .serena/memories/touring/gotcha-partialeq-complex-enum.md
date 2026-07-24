# GOTCHA: PartialEq Required for Enum with Complex Variants

## Problem
When implementing enum variants that contain non-`PartialEq` types (like `Vec<PathBuf>`), the derived `PartialEq` fails to compile.

## Error Example
```rust
#[derive(PartialEq)]
pub enum BlastRadiusOutput {
    Files(Vec<PathBuf>),  // PathBuf is PartialEq
    Rich(BlastRadius),      // BlastRadius may not be PartialEq
}
```

## Fix
Ensure all contained types implement `PartialEq`, or implement manually:

```rust
#[derive(PartialEq)]
pub struct BlastRadius { /* fields */ }

impl PartialEq for BlastRadiusOutput {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Files(a), Self::Files(b)) => a == b,
            (Self::Rich(a), Self::Rich(b)) => a == b,
            _ => false,
        }
    }
}
```

## Rule
When creating enum wrappers for type unification, verify ALL contained types derive or implement `PartialEq` before using `#[derive(PartialEq)]`.
