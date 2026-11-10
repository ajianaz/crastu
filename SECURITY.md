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
