use anyhow::Result;
use clap::Parser;
use env_logger::{Target, WriteStyle};
use log::LevelFilter;
use std::io::Write;

use crate::{cli::parser::Cli, execute_subcommand, handle_server_connection};

pub fn start_cli() -> Result<()> {
    let args = Cli::parse();
    setup_logging(args.verbose);

    if let Err(err) = match args {
        Cli {
            server: Some(uri),
            remote_cmd,
            conn_flags,
            ..
        } => handle_server_connection(uri, remote_cmd, conn_flags),
        Cli {
            subcommand: Some(cmd),
            ..
        } => {
            if let Some(output) = execute_subcommand(cmd)? {
                print!("{}", output);
            }

            Ok(())
        }
        _ => Ok(()),
    } {
        eprintln!("{}", err);
    }

    Ok(())
}

#[inline]
fn setup_logging(verbose: u8) {
    let log_level = match verbose {
        0 => LevelFilter::Warn,
        1 => LevelFilter::Info,
        _ => LevelFilter::Debug,
    };
    env_logger::builder()
        .filter(None, log_level)
        .write_style(WriteStyle::Auto)
        .target(Target::Stdout)
        .format(|buf, record| {
            let lvl = record.level();
            let color = buf.default_level_style(lvl);
            writeln!(
                buf,
                "[{}{}{} {}] {}",
                color.render(),
                lvl,
                color.render_reset(),
                record.target(),
                record.args(),
            )
        })
        .init();
}
