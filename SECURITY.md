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

- The host trusts processes that run as the same OS user, because they can already read and change Crastu's data on disk. It does not trust other users: the host socket and the files the host creates (database, journal, lock) are mode 0600 from the moment they exist, and their directories must belong to the user (a missing one is created with mode 0700). Directories owned by someone else, such as `/tmp`, and symlinks are refused, and there is no fallback to a shared temp directory.
- The agent name an MCP client declares is a label, not an identity. Per-agent overrides apply only to an agent that presents its token (`crastu agents add`, [#15](https://github.com/ajianaz/crastu/issues/15)); a caller without a token gets the stricter of the default and any override under the name it declares. The token sits in the client's configuration `env` block, a plain file that other processes of the same user can read: pairing stops a process from claiming an agent by name, not a process that can read that agent's config.
