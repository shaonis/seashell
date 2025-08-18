use std::fmt::{self, Display, Write};

use derive_more::{Display as AutoDisplay, From as AutoFrom};
use indexmap::IndexMap;
use itertools::{Either, Itertools};
use owo_colors::{OwoColorize, Style, Styled};

use crate::storage::config::{Scope, ScopedServer, Server, ServerEntry};


const SCOPE: Style = Style::new().bright_magenta();
const SERVER: Style = Style::new().bright_green();
const ATTR: Style = Style::new().blue();
const VALUE: Style = Style::new().white();
const SCOPE_SUFFIX: Styled<&'static str> = SCOPE.style("/");
const SERVER_SUFFIX: Styled<&'static str> = SERVER.style(":");
const ATTR_SUFFIX: Styled<&'static str> = ATTR.style(":");
const TREE_BRANCH: Styled<&'static str> = ATTR.style("├─╴");
const LAST_BRANCH: Styled<&'static str> = ATTR.style("└─╴");
const INDENT: &str = "  ";
const SCOPE_INDENT: &str = "    ";

/// Output data variants for different CLI commands
#[derive(AutoFrom, AutoDisplay)]
pub enum OutputData {
    LsCmd(LsResult),
    TestCmd(TestResult),
    #[display("")]
    None,
}

/// Result data for the list command
pub enum LsResult {
    /// Mixed list of scopes and global servers
    All(IndexMap<String, ServerEntry>),
    /// Only scopes (optionally prints default scope first)
    AllScopes(Option<Scope>, IndexMap<String, Scope>),
    /// A single named scope with its servers
    Scope(String, IndexMap<String, ScopedServer>),
}

/// Result data for the test command
#[derive(AutoFrom)]
pub struct TestResult(Result<Box<str>, String>);

impl Display for LsResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All(server_entries) => {
                let (scopes, servers): (Vec<_>, Vec<_>) = server_entries
                    .into_iter()
                    .partition_map(|(name, entry)| match entry {
                        ServerEntry::Scope(scope) => Either::Left((name , scope)),
                        ServerEntry::Global(server) => Either::Right((name, server)),
                    });
                for (scope, servers) in scopes.iter() {
                    print_scoped_servers(f, scope, servers)?;
                }
                let servers_exist = !servers.is_empty();
                if !scopes.is_empty() && servers_exist {
                    writeln!(f)?;
                }
                if servers_exist {
                    print_servers(f, &servers, true)?;
                }
            },
            Self::AllScopes(default, scopes) => {
                if let Some(default) = default {
                    default.print(f, "")?;
                    writeln!(f)?;
                }
                for (name, scope) in scopes {
                    writeln!(
                        f, "{}{}\n{}",
                        name.style(SCOPE), SCOPE_SUFFIX, scope,
                    )?;
                }
            },
            Self::Scope(scope, servers) => {
                print_scoped_servers(f, scope, servers)?;
            },
        }

        Ok(())
    }
}

impl Display for TestResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Ok(path) => write!(f, "The configuration file {} syntax is ok", path),
            Err(e) => write!(f, "{}", e),
        }
    }
}

impl Scope {
    fn print(&self, f: &mut impl Write, indent: &'static str) -> fmt::Result {
        let Self {
            user,
            port,
            known_hosts,
            private_key,
            openssh_cert,
        } = self;
        let capacity = user.is_some() as usize
            + port.is_some() as usize
            + known_hosts.is_some() as usize
            + private_key.is_some() as usize
            + openssh_cert.is_some() as usize;
        let mut fields: Vec<(&'static str, &dyn Display)> = Vec::with_capacity(capacity);

        if let Some(user) = user {
            fields.push(("user", user));
        }
        if let Some(port) = port {
            fields.push(("port", port));
        }
        let known_hosts_binding;
        if let Some(known_hosts) = known_hosts {
            known_hosts_binding = known_hosts.display();
            fields.push(("known_hosts", &known_hosts_binding));
        }
        let private_key_binding;
        if let Some(private_key) = private_key {
            private_key_binding = private_key.display();
            fields.push(("private_key", &private_key_binding));
        }
        let openssh_cert_binding;
        if let Some(openssh_cert) = openssh_cert {
            openssh_cert_binding = openssh_cert.display();
            fields.push(("openssh_cert", &openssh_cert_binding));
        }

        print_attributes(f, &fields, indent, true)
    }
}

impl Server {
    fn print(&self, f: &mut impl Write, indent: &'static str) -> fmt::Result {
        let Self {
            address,
            scope,
        } = self;
        let fields: Vec<(&'static str, &dyn Display)> = vec![("address", address)];
        let has_scope_fields = !scope.is_empty();
        print_attributes(f, &fields, indent, !has_scope_fields)?;
        if has_scope_fields {
            scope.print(f, indent)?;
        }

        Ok(())
    }
}

impl Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.print(f, INDENT)
    }
}

#[inline]
fn print_scoped_servers(
    f: &mut impl Write,
    scope: &str,
    servers: &IndexMap<String, ScopedServer>,
) -> fmt::Result {
    writeln!(f, "{}{}", scope.style(SCOPE), SCOPE_SUFFIX)?;
    print_servers(f, &servers.iter().collect::<Vec<_>>(), false)
}

#[inline]
fn print_servers(
    f: &mut impl Write,
    servers: &[(&String, &ScopedServer)],
    global: bool,
) -> fmt::Result {
    let name_indent = if global { "" } else { INDENT };
    let attr_indent = if global { INDENT } else { SCOPE_INDENT };

    for (name, server) in servers {
        if let ScopedServer::Address(address) = server {
            writeln!(
                f, "{}{}{} {}",
                name_indent,
                name.style(SERVER),
                SERVER_SUFFIX,
                address.style(VALUE),
            )?;
        }
    }
    for (name, server) in servers {
        if let ScopedServer::Override(inner) = server {
            writeln!(f, "{}{}{}", name_indent, name.style(SERVER), SERVER_SUFFIX)?;
            inner.print(f, attr_indent)?;
            writeln!(f)?;
        }
    }

    Ok(())
}

fn print_attributes(
    f: &mut impl Write,
    fields: &[(&str, &dyn Display)],
    indent: &'static str,
    is_last_group: bool,
) -> fmt::Result {
    if indent.is_empty() {
        for (key, value) in fields.iter() {
            writeln!(
                f, "{}{} {}",
                key.style(ATTR), ATTR_SUFFIX, value.style(VALUE),
            )?;
        }
        return Ok(())
    }
    let mut tree_attrs = |attrs: &[(&str, &dyn Display)]| -> fmt::Result {
        for (key, value) in attrs {
            writeln!(f, "{}{}{}{} {}",
                indent,
                TREE_BRANCH,
                key.style(ATTR),
                ATTR_SUFFIX,
                value.style(VALUE),
            )?;
        }

        Ok(())
    };
    if is_last_group {
        if let Some((last, rest)) = fields.split_last() {
            tree_attrs(rest)?;
            let (key, value) = last;
            write!(f, "{}{}{}{} {}",
                indent,
                LAST_BRANCH,
                key.style(ATTR),
                ATTR_SUFFIX,
                value.style(VALUE),
            )?;
        }
    } else {
        tree_attrs(fields)?;
    }

    Ok(())
}
