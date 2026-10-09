# Security Policy

DBench handles live database connections and credentials. Security is not an afterthought — it is a first-class requirement embedded into every layer of the application.

---

## Reporting a Vulnerability

**Do NOT open a public GitHub issue for security vulnerabilities.**

Please report security vulnerabilities via:

- **Email**: hadihaider.business@gmail.com (PGP key available at `/pgp-key.asc`)
- **GitHub Security Advisory**: [Report a vulnerability](https://github.com/hadihaider055/catalyst-dbench/security/advisories/new)

Please include:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

I follow [responsible disclosure](https://en.wikipedia.org/wiki/Responsible_disclosure) and will credit you in the release notes unless you prefer to stay anonymous.

---

## Supported Versions

| Version        | Security Fixes      |
| -------------- | ------------------- |
| latest stable  | Yes                 |
| previous minor | Yes (critical only) |
| older          | No                  |

---

## Security Architecture Overview

The security architecture is implemented in `crates/dbench-security/` — see the source files for full design details.

### 7 Security Layers

```
Layer 1 — Credential Security    (OS keychain, never plaintext)
Layer 2 — Transport Security     (TLS 1.2+, SSH tunnels, cert pinning)
Layer 3 — Memory Security        (zeroize, Secret<T> wrappers, no sensitive logs)
Layer 4 — Application Security   (Tauri CSP, IPC validation, input sanitization)
Layer 5 — Data Security          (column masking, read-only modes, no query injection)
Layer 6 — Audit & Compliance     (structured audit log, tamper-evident)
Layer 7 — Secrets Integration    (Vault, AWS Secrets Manager, 1Password)
```

---

## Key Security Properties

- **Credentials are never stored in plaintext.** All passwords and tokens are stored in the OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service / KWallet).
- **No telemetry without explicit opt-in.** Zero data leaves your machine by default.
- **Memory-safe core.** The Rust type system eliminates entire classes of vulnerabilities (buffer overflows, use-after-free, data races).
- **Sensitive values are zeroed from memory** when dropped, using the `zeroize` and `secrecy` crates.
- **TLS is enforced by default.** Disabling TLS requires an explicit opt-out with a visible warning.
- **No query string injection.** All database operations use parameterized queries/prepared statements.
- **Audit logging** records every query executed, every connection made, with timestamps and user context.
- **Read-only connection mode.** Connections can be marked read-only, preventing any write operations at the driver level.
