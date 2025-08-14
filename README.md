# Seashell

User-friendly and modern SSH client written in pure Rust (sea noise inside)

### What You Should Know

#### What Seashell is NOT

- `shh` isn’t trying to replace the standard `ssh` utility
- The goal is to make SSH more convenient for everyday use
- It’s built with modern systems and protocols in mind
- It was created by the author to solve their own problems (and maybe yours too)

---
#### Why Use Seashell?

- **Pure Rust SSH implementation** — powered by the [russh](https://github.com/Eugeny/russh) crate, so it doesn’t depend on external system packages like OpenSSH or libssh
- **Flexible, user-friendly YAML config** — no more messy duplication of connection details
- **Scopes** — a way to group shared connection settings for multiple servers, while keeping their own isolated «namespace» of hostnames
- **Full regex support** for matching hosts and names
- **Placeholders** — drop an alias into a pre-defined server address (e.g. vm101 → vm101.anyway.local)
- **Command-based config management** — run CRUD (Create, Read, Update, Delete) operations directly from your terminal without editing files manually

---
### Configuration Basics

Your config has three main sections:

- default connection settings
- `scopes` — definitions of each scope
- `servers` — definitions of each server

---
#### What’s a Scope?

- A scope is a bundle of all connection settings except the host address
- Each scope has its own namespace for servers — meaning you can reuse the same aliases if they belong to different scopes
- Scopes cannot be nested
- There’s also a global scope, which can hold default connection settings (e.g. user, port, path to known_hosts)

---
#### What’s a Server?

- A server is just the address you connect to
- To connect to a server in a scope, you first switch to that scope

Servers can be:
- Just an address
- An address with its own settings that override the scope defaults

---
#### Example Config

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

---
### Misc

The author works on the project when he has the desire and time. The project is also open to pull requests

#### TODO

- **Non-interactive mode** — run commands without entering an interactive shell

---
#### Ideas

- **Split config into multiple files** for easier organization
- **Jump hosts** — one bastion per scope (either directly inside the scope or via chaining scope-to-bastion)
- **Interactive server selection** when a bastion host is present
- **Hooks** — automatically run commands on connect/disconnect from a server
- **3rd-party integrations** — easily fetch secrets from external secret managers
