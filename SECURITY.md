# Security Policy

## Supported versions

Only the latest release receives security fixes.

| Version | Supported |
|---|---|
| 0.1.x | ✓ |

## Reporting a vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Send a report to **lightweb.it@gmail.com** with:

- A description of the vulnerability and its potential impact
- Steps to reproduce (or a proof-of-concept if applicable)
- The version of Pragma affected

You can expect an acknowledgement within 72 hours and a status update within 7 days.

## Known limitations

**Prompt files are stored as plain text.** External prompts saved in `~/.pragma/prompts/<label>.txt` are not encrypted at rest. If your prompts contain sensitive information, ensure the `~/.pragma/` directory has appropriate filesystem permissions. Encryption at rest is planned for a future release.

## Scope

Issues in scope include anything in this repository: the Tauri backend (`src-tauri/`), the parser crate (`pragma-parser/`), the CLI wrapper (`pragma-cli/`), and the Vue frontend (`src/`).

Out of scope: vulnerabilities in Claude Code itself, in Tauri, or in other upstream dependencies. Report those to their respective maintainers.

## Disclosure policy

Once a fix is ready and released, the vulnerability will be disclosed in the `CHANGELOG.md` under a **Security** section. Credit will be given to the reporter unless they prefer to remain anonymous.
