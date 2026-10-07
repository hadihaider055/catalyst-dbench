# Contributing to Catalyst DBench

Thank you for your interest in contributing! DBench is an ambitious open-source project and every contribution matters.

## Getting Started

1. **Fork** the repository on GitHub
2. **Clone** your fork: `git clone https://github.com/YOUR-USERNAME/catalyst-dbench.git`
3. **Install dependencies** (see README.md Build from Source)
4. **Create a branch**: `git checkout -b feat/your-feature-name`
5. **Make changes**, write tests
6. **Run checks**: `cargo test --workspace && cargo clippy --workspace -- -D warnings`
7. **Submit a Pull Request**

## Commit Message Convention

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add MySQL driver
fix: correct TLS certificate validation on Windows
docs: update CONTRIBUTING guide
chore: bump tokio to 1.36
refactor: extract connection pool into separate module
test: add integration tests for PostgreSQL driver
security: harden SSH key loading to reject world-readable keys
```

## Adding a New Database Driver

See [CLAUDE.md](CLAUDE.md#adding-a-new-database-driver) for the step-by-step guide.

## Code Review

- All PRs require at least one approval
- Security-related changes require review from a maintainer
- CI must pass (tests, clippy, audit)

## Questions?

Open a [GitHub Discussion](https://github.com/hadihaider055/catalyst-dbench/discussions) for questions and ideas.
