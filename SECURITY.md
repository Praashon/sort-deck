# Security Policy

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities, path-traversal bugs, sandbox bypasses, or data-loss defects.

Instead, report vulnerabilities privately via GitHub Security Advisories ("Report a vulnerability" in the repository's **Security** tab) or contact the project maintainers directly once a security contact address is published.

Please include:

- A clear description of the vulnerability and affected component.
- Minimal steps or a sandboxed test case to reproduce the issue.
- Potential impact (e.g., arbitrary path access, overwrite, privilege/UI injection).

## Supported Versions

SortDeck is currently in pre-release development (Phase 0). Security fixes target the `main` branch until v1.0.0 is released.

## Security Architecture

See [docs/SAFETY.md](./docs/SAFETY.md) and [AGENTS.md](./AGENTS.md) §9 for the Tauri capability model, CSP policy, backend path validation, and threat model.
