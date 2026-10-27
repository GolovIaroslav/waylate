# Security Policy

## Reporting

Report security issues privately through GitHub Security Advisories.

Do not open public issues for API key leaks, command execution bugs, or local file exposure.

## Scope

Waylate handles clipboard text, local config, optional SQLite history, and optional API keys through the system keyring. Network translation providers and the AI assistant are disabled by default. AI keys are saved via Secret Service/KWallet, not in the JSON config. For AI, only explicit submissions can send questions and optional source-text context to configured endpoints. Do not expose untrusted local HTTP servers or transmit confidential text to cloud services without reviewing their data handling and terms.
