# Insight: parking_lot RwLock vs std::sync::Mutex

## When Mutex is Bottleneck
SharedPipeline was identified as a mutex bottleneck in touring-ast.

## parking_lot::RwLock Advantages
- `RwLock` allows multiple readers OR single writer
- `parking_lot` provides:
  - No poisoning (threads panic, lock is still accessible)
  - Faster uncontended operations
  - Fair scheduling options (FIFO)
  - `read()` returns `RwLockReadGuard` (no RAII clone cost)

## Comparison
| Aspect | `std::Mutex` | `parking_lot::RwLock` |
|--------|--------------|----------------------|
| Readers | 1 at a time | Multiple concurrent |
| Writer | 1 at a time | 1 at a time (blocks readers) |
| Uncontended cost | Higher | Lower |
| Poisoning | Yes (panics) | No |
| Try operations | Yes | Yes |

## Recommendation for SharedPipeline
Consider replacing `std::sync::Mutex<SharedPipeline>` with `parking_lot::RwLock<SharedPipeline>` if read-heavy workload.
