use std::fmt::{self, Display, Write};

use derive_more::{Display as AutoDisplay, From as AutoFrom};
use indexmap::IndexMap;
use itertools::{Either, Itertools};
use owo_colors::{OwoColorize, Style};

use crate::storage::config::{Scope, ScopedServer, Server, ServerEntry};


const SCOPE: Style = Style::new().bright_magenta();
const ATTR: Style = Style::new().blue();
const VALUE: Style = Style::new().white();


#[derive(AutoFrom, AutoDisplay)]
pub enum OutputData {
    LsCmd(LsResult),
    TestCmd(TestResult),
    #[display("")]
    None,
}

pub enum LsResult {
    All(IndexMap<String, ServerEntry>),
    AllScopes(Option<Scope>, IndexMap<String, Scope>),
    Scope(String, IndexMap<String, ScopedServer>),
}

#[derive(AutoFrom)]
pub struct TestResult(Result<Box<str>, String>);

impl Display for LsResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut output = String::new();
        
        match self {
            Self::All(server_entries) => {
                let (scopes, servers): (Vec<_>, Vec<_>) = server_entries
                    .iter()
                    .partition_map(|(name, entry)| match entry {
                        ServerEntry::Scope(scope) => Either::Left((name , scope)),
                        ServerEntry::Global(server) => Either::Right((name, server)),
                    });
                for (name, scope) in scopes {
                    print_scoped_servers(&mut output, name, scope)?;
                }
                if !servers.is_empty() {
                    writeln!(output)?;
                    for (name, server) in servers {
                        writeln!(output, "{}{}", name.bright_green(), ":".bright_green())?;
                        server.print(&mut output, 2)?;
                        writeln!(output)?;
                    }
                }
            },
            Self::AllScopes(default, scopes) => {
                if let Some(default) = default {
                    default.print(&mut output, 0)?;
                    writeln!(output)?;
                }
                for (name, scope) in scopes {
                    writeln!(
                        output, "{}{}\n{}",
                        name.style(SCOPE), "/".style(SCOPE), scope,
                    )?;
                }
            },
            Self::Scope(name, servers) => {
                print_scoped_servers(&mut output, name, servers)?;
            },
        }

        write!(f, "{}", output)
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

impl Server {
    fn print(&self, f: &mut impl Write, shift: usize) -> fmt::Result {
        let Self {
            address,
            scope,
        } = self;
        let fields: Vec<(&'static str, &dyn Display)> = vec![("address", address)];
        print_attributes(f, &fields, shift, false)?;

        scope.print(f, shift)
    }
}

impl Scope {
    fn print(&self, f: &mut impl Write, shift: usize) -> fmt::Result {
        let Self {
            user,
            port,
            known_hosts,
            private_key,
            openssh_cert,
        } = self;
        let mut fields: Vec<(&'static str, &dyn Display)> = Vec::with_capacity(5);

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

        print_attributes(f, &fields, shift, true)
    }
}

impl Display for ScopedServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(address) => write!(f, "{}", address.style(VALUE)),
            Self::Override(server) => server.print(f, 4),
        }
    }
}

impl Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.print(f, 2)
    }
}

#[inline]
fn print_scoped_servers(output: &mut impl Write, name: &str, servers: &IndexMap<String, ScopedServer>) -> fmt::Result {
    writeln!(output, "{}{}", name.style(SCOPE), "/".style(SCOPE))?;
    for (name, server) in servers.iter()
        .filter(|(_, server)| matches!(server, ScopedServer::Address(_)))
    {
        writeln!(output, "  {}{} {}", name.bright_green(), ":".bright_green(), server)?;
    }
    for (name, server) in servers.iter()
        .filter(|(_, server)| matches!(server, ScopedServer::Override(_)))
    {
        writeln!(output, "  {}{}\n{}", name.bright_green(), ":".bright_green(), server)?;
    }

    Ok(())
}

#[inline]
fn print_attributes(
    f: &mut impl Write,
    fields: &[(&str, &dyn Display)],
    shift: usize,
    last: bool,
) -> fmt::Result {
    if shift == 0 {
        for (key, value) in fields.iter() {
            writeln!(
                f, "{}{} {}",
                key.style(ATTR), ":".style(ATTR), value.style(VALUE),
            )?;
        }
        return Ok(())
    }
    let last_index = fields.len().saturating_sub(1);
    let shift = " ".repeat(shift);
    let start_range = if last { ..last_index } else { ..last_index + 1 };

    for (key, value) in fields[start_range].iter() {
        writeln!(
            f, "{}{}{}{} {}",
            shift, "├─╴".style(ATTR), key.style(ATTR), ":".style(ATTR), value.style(VALUE),
        )?;
    }
    if !last {
        return Ok(())
    }
    let (key, value) = fields[last_index];
    write!(
        f, "{}{}{}{} {}",
        shift, "└─╴".style(ATTR), key.style(ATTR), ":".style(ATTR), value.style(VALUE),
    )?;

    Ok(())
}
