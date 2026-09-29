# Contributing to nautilus-schwab

Thank you for your interest in contributing! This document provides guidelines for reporting bugs, requesting features, and submitting pull requests.

## Reporting Bugs

Before filing a bug report, please:

1. **Search existing issues** to avoid duplicates
2. **Verify the bug is reproducible** with the latest version on `main`
3. **Gather relevant information**:
   - Rust version (`rustc --version`)
   - Python version (if applicable)
   - Operating system
   - Minimal reproduction steps
   - Expected vs actual behavior
   - Relevant log output (with credentials redacted)

File bug reports via [GitHub Issues](https://github.com/satr-trading/nautilus-schwab/issues) with the **Bug Report** template.

### Security Issues

**Do not file public issues for security vulnerabilities.** Instead, email the maintainers directly or use GitHub's private vulnerability reporting feature. Never include API keys, tokens, or other credentials in any communication.

## Feature Requests

We welcome feature requests! Please:

1. **Check the roadmap** in [README.md](README.md#roadmap) — your idea may already be planned
2. **Describe the use case**, not just the solution — explain *why* this matters
3. **Consider scope** — small, focused features are more likely to be accepted than large overhauls

File feature requests via GitHub Issues with the **Feature Request** template.

## Pull Requests

### Before You Start

- For significant changes, **open an issue first** to discuss the approach
- Keep PRs focused — one feature or fix per PR
- Ensure all tests pass: `cargo test`

### Development Setup

```bash
git clone https://github.com/satr-trading/nautilus-schwab.git
cd nautilus-schwab
cargo build
cargo test
```

For Python bindings development:
```bash
pip install maturin
maturin develop
```

### Coding Standards

- **Rust**: Follow standard Rust conventions. Run `cargo fmt` and `cargo clippy` before committing.
  ```bash
  cargo fmt --all
  cargo clippy --all-targets -- -D warnings
  ```
- **Python**: Follow PEP 8. Type hints are required for public APIs.
- **Tests**: Add unit tests for new functionality. Integration tests require live Schwab credentials and should be gated behind environment checks.
- **Documentation**: Update doc comments for public items. Update README.md if user-facing behavior changes.
- **Credentials**: Never commit secrets, tokens, or API keys. Use `.env.example` as a template.

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]

[optional footer]
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `chore`, `ci`, `perf`

Examples:
- `feat(data): implement historical bar response conversion`
- `fix(execution): correct order status mapping for partial fills`
- `test: add streamer reconnection edge cases`

### PR Checklist

Before requesting review:

- [ ] All tests pass (`cargo test`)
- [ ] Code is formatted (`cargo fmt --all`)
- [ ] No clippy warnings (`cargo clippy --all-targets -- -D warnings`)
- [ ] New functionality has tests
- [ ] Documentation updated if needed
- [ ] No credentials or secrets committed
- [ ] CHANGELOG.md updated (under "Unreleased")

## Project Structure

See [README.md](README.md#project-structure) for the source layout. Key directories:

- `src/common/` — shared types (credentials, symbols, enums)
- `src/oauth/` — token provider and OAuth flow helpers
- `src/http/` — HTTP client wrapper and error mapping
- `src/data.rs` — DataClient implementation with streamer
- `src/execution.rs` — ExecutionClient implementation
- `src/factories.rs` — Nautilus factory implementations
- `src/python/` — PyO3 Python bindings
- `tests/` — integration tests

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
