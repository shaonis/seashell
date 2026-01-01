# 🐚 Seashell

User-friendly and modern SSH client for everyday use written in pure Rust.

Not just another SSH key manager or wrapper for the standard `ssh` utility, but a **self-sufficient client**

## What you should know

### What Seashell is not

- `shh` isn’t trying to replace the standard `ssh` utility
- The goal is to make SSH more convenient for everyday use
- It’s built with modern systems and protocols in mind

### Why use Seashell?

- **Pure Rust SSH implementation** — powered by the [russh](https://github.com/Eugeny/russh) crate, so it doesn’t depend on external system packages like OpenSSH or libssh
- **Flexible, user-friendly YAML config** — no more messy duplication of connection details
- **Scopes** — a way to group shared connection settings for multiple servers, while keeping their own isolated «namespace» of hostnames
- **Full regex support** — for matching hosts and names
- **Placeholders** — drop an alias into a pre-defined server address (e.g. `vm101` → `vm101.anyway.local`)
- ease of creating your own add-ons and more

## Installation

Currently, only Unix-like systems are supported

### 1. Download executable file

1. Go to the [Releases](https://github.com/shaonis/seashell/releases) tab and download the desired version
2. Unpack the archive `tar -xvzf ./archive_name.tar.gz`
3. Make it executable `chmod +x ./shh`
4. Move it to PATH `sudo mv ./shh /usr/bin/`

### 2. Build from sources

Required Rust version >= `1.88.0`

```sh
# Install Rust via rustup (first, try to find rustup in your distro's repo)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Build seashell
git clone https://github.com/shaonis/seashell.git
cd ./seashell
cargo build --release
# Add to PATH (to run from any directory)
sudo cp ./target/release/shh /usr/bin/shh
```

## Configuration basics

The configuration is located in `~/.shh/config.yml` (not `ssh`) and consists of three main sections:
- default connection settings
- `scopes` — each scope definition
- `servers` — each server definition

### What’s a scope?

- A `scope` is a bundle of all connection settings except the host address
- Each scope has its own namespace for servers — meaning you can reuse the same aliases if they belong to different scopes
- Scopes cannot be nested
- There’s also a global scope, which can hold default connection settings (e.g. `user`, `port`, path to `known_hosts`)

#### Scope syntax

**Note:** All fields in `scope` are optional

```yaml
# User to connect as
user: string (default - current system user)
# Port to connect to
port: integer (0 to 65535, default - 22)
# Path to the known hosts file
known_hosts: /path/to/known_hosts
# Path to the private key
private_key: /path/to/private_key
# Path to the OpenSSH certificate
openssh_cert: /path/to/openssh_cert
# Preferred key exchange algorithms
kex: list (CSV for CLI, list for YAML)
# Preferred host & public key algorithms
alg: list (CSV for CLI, list for YAML)
# Preferred symmetric ciphers
cipher: list (CSV for CLI, list for YAML)
# Preferred MAC algorithms
mac: list (CSV for CLI, list for YAML)
# Set the time to wait for a connection
timeout: integer (seconds)
# Duration between keepalive messages if the server is silent
interval: integer (seconds)
# Maximum number of keepalives allowed without a response
retries: integer
```

### What’s a server?

- A server is just the address you connect to
- To connect to a server in a scope, you first switch to that scope (`shh use <scope_name>`)

Servers can be:

- Just an address
- An address with its own settings that override the scope defaults

#### Server syntax

**Note:** Within the server, you can override any field that is present in the `scope` syntax

```yaml
servers:
  my_server1: 42.101.146.256
  my_server2: 71.25.23.256

# or overriding scope defaults

servers:
  my_server2:
    address: 42.101.146.256
    port: 2222
    private_key: ~/.shh/my_server2.key

# you can also combine both styles

servers:
  my_server1: 42.101.146.256
  my_server2:
    address: 71.25.23.256
    port: 2222
    private_key: ~/.shh/my_server2.key
```

**Tip:** If possible, specify the `server` directly as the address, placing all other connection parameters in the external `scope`

### Example config

**Note:** In the `servers` section, a block name should match its scope name if you want to list servers inside that scope.
If the block doesn't look like a scope, it’s treated as a server in the **default (global) scope**

```yaml
user: admin
private_key: ~/.shh/test.key
known_hosts: /dev/null

scopes:
  mine:
    private_key: ~/.shh/mine/default.key
    known_hosts: ~/.shh/mine/known_hosts

  work:
    port: 2332
    user: full_name
    private_key: ~/.shh/work/default.key
    known_hosts: ~/.shh/work/known_hosts

servers:
  mine:
    tars: 42.101.146.256
    icarus: 71.25.23.256

  work:
    test:
      address: test.anyway.local
      private_key: ~/.shh/work/test.key
    ldap:
      address: ldap.anyway.local
      private_key: ~/.shh/work/ldap.key
    stats:
      address: stats.anyway.local
      private_key: ~/.shh/work/stats.key
    "vm-[0-9]*": $h.anyway.local

  test: test.local
```

You can see that regular expressions and the placeholder `$h` are used here. Instead of `$h`, the actual host name (alias) that you entered is substituted (`vm101` → `vm-101.anyway.local`)

## Compatibility

- Despite the fact that [russh](https://github.com/Eugeny/russh) supports various algorithms and ciphers, `seashell` uses a more limited range of the most stable and secure ones
- It was decided to remove the compression option (`-C`), as it is largely redundant and in most cases it is better to do without it

### Supported

<details>
<summary>Authentication methods</summary>

### In order of priority

- Via SSH agent (e.g. for SSO, [KeyPassXC](https://keepassxc.org/), [Vaultwarden](https://www.vaultwarden.net/)/[Bitwarden](https://bitwarden.com/), [1Password](https://1password.com/)... integrations)
- With OpenSSH certificate
- Classic public key
- Keyboard-interactive mode
- Simple password
- None
</details>

<details>
<summary>Key exchange algorithms</summary>

### `--kex` flag

- `mlkem768x25519-sha256`
- `curve25519-sha256`
- `curve25519-sha256@libssh.org`
- `diffie-hellman-group-exchange-sha256`
- `diffie-hellman-group18-sha512`
- `diffie-hellman-group17-sha512`
- `diffie-hellman-group16-sha512`
- `diffie-hellman-group15-sha512`
- `diffie-hellman-group14-sha256`
- `ext-info-c`
- `ext-info-s`
- `kex-strict-c-v00@openssh.com`
- `kex-strict-s-v00@openssh.com`
</details>

<details>
<summary>Host & public key algorithms</summary>

### `--alg` flag

- `ssh-ed25519`
- `ecdsa-sha2-nistp256`
- `ecdsa-sha2-nistp384`
- `ecdsa-sha2-nistp521`
- `rsa-sha2-512`
- `rsa-sha2-256`
- `ssh-rsa`
</details>

<details>
<summary>Symmetric ciphers</summary>

### `--chipher` flag

- `chacha20-poly1305@openssh.com`
- `aes256-gcm@openssh.com`
- `aes256-ctr`
- `aes192-ctr`
- `aes128-ctr`
</details>

<details>
<summary>MAC algorithms</summary>

### `--mac` flag

- `hmac-sha2-512-etm@openssh.com`
- `hmac-sha2-256-etm@openssh.com`
- `hmac-sha2-512`
- `hmac-sha2-256`
- `hmac-sha1-etm@openssh.com`
- `hmac-sha1`
</details>

## Misc

- The project is also **open to pull requests**
- The author works on the project when he has the desire and time

### Ideas

## TODO

- **TUI** — terminal user interface for easier server management
- **Audit** — server vulnerability scanning, policy checking
- **Split config into multiple files** for easier organization
- **Jump hosts** — one bastion per scope (either directly inside the scope or via chaining scope-to-bastion)
- **Interactive server selection** when a bastion host is present
- **Hooks** — automatically run commands on connect/disconnect from a server

## Maybe, maybe

- **UDP support?** (idea from [mossh](https://github.com/mobile-shell/mosh))
- **Cross-platform?** (e.g. Windows requires an alternative implementation of async descriptors)
