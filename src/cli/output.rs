use std::fmt::{self, Display, Write};

use crate::storage::config::{Scope, ScopedServer, Server, ServerEntry};
use indexmap::IndexMap;
use itertools::{Either, Itertools};
use owo_colors::{OwoColorize, Style, Styled};

macro_rules! define_styles {
    ($($name:ident: $color:ident),* $(,)?) => {
        $(const $name: Style = Style::new().$color();)*
    };
}

define_styles! {
    SCOPE: bright_magenta,
    SERVER: bright_green,
    ATTR: blue,
    VALUE: white,
}

macro_rules! styled_const {
    ($name:ident, $style:expr, $value:expr) => {
        const $name: Styled<&'static str> = $style.style($value);
    };
}

styled_const!(SCOPE_SUFFIX, SCOPE, "/");
styled_const!(SERVER_SUFFIX, SERVER, ":");
styled_const!(ATTR_SUFFIX, ATTR, ":");
styled_const!(TREE_BRANCH, ATTR, "├─╴");
styled_const!(LAST_BRANCH, ATTR, "└─╴");

const INDENT: &str = "  ";
const SCOPE_INDENT: &str = "    ";
const SCOPE_FIELD_CAPACITY: usize = 12;

/// Result data for the list command
pub enum LsOutput {
    /// Mixed list of scopes and global servers
    All(IndexMap<String, ServerEntry>),
    /// Only scopes (optionally prints default scope first)
    AllScopes(Option<Box<Scope>>, IndexMap<String, Scope>),
    /// A single named scope with its servers
    Scope(String, IndexMap<String, ScopedServer>),
}

/// Result data for the test command
pub struct TestOutput(pub Result<Box<str>, String>);

impl Display for LsOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All(server_entries) => {
                let (scopes, servers): (Vec<_>, Vec<_>) =
                    server_entries
                        .into_iter()
                        .partition_map(|(name, entry)| match entry {
                            ServerEntry::Scope(scope) => Either::Left((name, scope)),
                            ServerEntry::Global(server) => Either::Right((name, server)),
                        });
                for (scope, servers) in scopes.iter() {
                    print_scoped_servers(f, scope, servers)?;
                }
                if !servers.is_empty() {
                    if !scopes.is_empty() {
                        writeln!(f)?;
                    }
                    print_servers(f, &servers, true)?;
                }
            }
            Self::AllScopes(default, scopes) => {
                if let Some(default_scope) = default {
                    default_scope.print(f, "")?;
                    if !scopes.is_empty() {
                        writeln!(f)?;
                    }
                }
                for (name, scope) in scopes {
                    writeln!(f, "{}{}\n{}", name.style(SCOPE), SCOPE_SUFFIX, scope)?;
                }
            }
            Self::Scope(scope, servers) => {
                print_scoped_servers(f, scope, servers)?;
            }
        }

        Ok(())
    }
}

impl Display for TestOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Ok(path) => write!(f, "The configuration file {} syntax is ok", path),
            Err(e) => write!(f, "{}", e),
        }
    }
}

/// Adds optional fields to a field list.
///
/// Variants:
/// - Default: adds field as-is if Some
/// - `as path`: converts PathBuf to display format
/// - `as list`: joins iterables with ", "
macro_rules! push_fields {
    ($fields:ident, { $($field:ident $(as $variant:ident)?),* $(,)? }) => {
        $(
            push_fields!(@inner $fields, stringify!($field), $field $(, $variant)?);
        )*
    };
    (@inner $fields:ident, $name:expr, $opt:expr) => {
        if let &Some(ref v) = $opt {
            $fields.push(($name, v));
        }
    };
    (@inner $fields:ident, $name:expr, $opt:expr, path) => {
        let binding;
        if let &Some(ref v) = $opt {
            binding = v.display();
            $fields.push(($name, &binding));
        }
    };
    (@inner $fields:ident, $name:expr, $opt:expr, list) => {
        let binding;
        if let &Some(ref v) = $opt {
            binding = v.iter().join(", ");
            $fields.push(($name, &binding));
        }
    };
}

impl Scope {
    fn print(&self, f: &mut impl Write, indent: &'static str) -> fmt::Result {
        let Self {
            user,
            port,
            known_hosts,
            private_key,
            openssh_cert,
            kex,
            alg,
            cipher,
            mac,
            timeout,
            interval,
            retries,
        } = self;
        let mut fields: Vec<(&'static str, &dyn Display)> =
            Vec::with_capacity(SCOPE_FIELD_CAPACITY);

        push_fields!(fields, {
            user,
            port,
            known_hosts as path,
            private_key as path,
            openssh_cert as path,
            kex as list,
            alg as list,
            cipher as list,
            mac as list,
            timeout,
            interval,
            retries,
        });

        print_attributes(f, &fields, indent, true)
    }
}

impl Server {
    fn print(&self, f: &mut impl Write, indent: &'static str) -> fmt::Result {
        let Self { address, scope } = self;
        let fields: Vec<(&'static str, &dyn Display)> = vec![("address", address)];
        let is_last = self.scope.is_empty();
        print_attributes(f, &fields, indent, is_last)?;
        if !is_last {
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
    let (name_indent, attr_indent) = if global {
        ("", INDENT)
    } else {
        (INDENT, SCOPE_INDENT)
    };

    for (name, server) in servers {
        if let ScopedServer::Address(address) = server {
            writeln!(
                f,
                "{}{}{} {}",
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
    is_last: bool,
) -> fmt::Result {
    if indent.is_empty() {
        for (key, value) in fields {
            writeln!(
                f,
                "{}{} {}",
                key.style(ATTR),
                ATTR_SUFFIX,
                value.style(VALUE)
            )?;
        }
        return Ok(());
    }
    let Some((last, rest)) = fields.split_last() else {
        return Ok(());
    };

    for (key, value) in rest {
        write_tree_attr(f, indent, key, value)?;
    }

    if is_last {
        write!(
            f,
            "{}{}{}{} {}",
            indent,
            LAST_BRANCH,
            last.0.style(ATTR),
            ATTR_SUFFIX,
            last.1.style(VALUE),
        )
    } else {
        write_tree_attr(f, indent, last.0, last.1)
    }
}

#[inline]
fn write_tree_attr(
    f: &mut impl Write,
    indent: &str,
    key: &str,
    value: impl Display,
) -> fmt::Result {
    writeln!(
        f,
        "{}{}{}{} {}",
        indent,
        TREE_BRANCH,
        key.style(ATTR),
        ATTR_SUFFIX,
        value.style(VALUE)
    )
}
