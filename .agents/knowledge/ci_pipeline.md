# CI Pipeline

## ⚡ Current State
GitHub Actions CI is configured at `.github/workflows/ci.yml`. Runs on `ubuntu-latest` with stable Rust.

Steps:
1. Checkout → Install Rust toolchain (with `rustfmt, clippy` components) → Cache dependencies
2. **Clippy**: `cargo clippy --workspace --all-targets --all-features`
3. **Tests**: `cargo test --workspace`

Notable decisions:
- **Removed `cargo fmt --all -- --check`** step — was causing too many CI failures and slowing iteration.
- Clippy is kept but may need `-- -D warnings` flag added later for stricter enforcement.

## 📖 History
### Update from transcript 1400a764-7e5b-4660-a54a-393596d48641
- User removed the formatting check step from CI after repeated failures.
- Pushed changes to `feature/ai-memory-system` branch.

### Update from transcript e2255da9-fd7e-449a-99ef-9eb7765ed471
- CI pipeline discussed alongside consistent_hash crate development.
- Clippy runs with `--workspace --all-targets --all-features`.
