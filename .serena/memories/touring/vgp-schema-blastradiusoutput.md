# VGP Schema: BlastRadiusOutput (Verified)

## Verified via touring_ast_find

### BlastRadiusOutput Enum
```rust
pub enum BlastRadiusOutput {
    Files(Vec<PathBuf>),
    Rich(BlastRadius),
}
```

### Methods
- `files(&self) -> Vec<PathBuf>` — extracts paths from any variant

### From Implementations
- `From<Vec<PathBuf>> for BlastRadiusOutput`
- `From<BlastRadius> for BlastRadiusOutput`

### PartialEq
- Both `BlastRadiusOutput` and `BlastRadius` derive `PartialEq`

### Usage Pattern
```rust
let output: BlastRadiusOutput = cache.blast_radius(symbol)?;
let files: Vec<PathBuf> = output.files();
```
