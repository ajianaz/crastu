# Security policy

## Supported versions

Crastu has no release yet. Security fixes land on `develop`.

## Reporting a vulnerability

Please report privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**. Do not open a public issue.

Include what you found, how to reproduce it, and the impact you expect. The maintainer will reply as soon as possible.

Areas that matter most:

- The local host and its IPC socket, which accept commands from other processes.
- The MCP server and its approval policy (read, change and delete permissions per agent).
- Stored provider keys (bring-your-own-key) and model downloads (checksums, sources).

## Threat model

- The host trusts processes that run as the same OS user, because they can already read and change Crastu's data on disk. It does not trust other users: the host socket and its directory are private to the user (modes 0600 and 0700).
- The agent name an MCP client declares is a label, not an identity. Until agents are paired with a token ([#15](https://github.com/ajianaz/crastu/issues/15)), per-agent overrides are a convenience, not a security boundary, and the defaults (Change and Delete set to Ask) apply to every name.
