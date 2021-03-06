use anyhow::{Context, Result};
use clap::Clap;
use colored::Colorize;
use dist_package_db::{
    database::{DistpacDB, MissingDBAction},
    models::PackageEntry,
};
use indicatif::{ProgressBar, ProgressStyle};
use log::debug;
use transmission_wrapper::{bytes::Bytes, Transmission, TransmissionOpts};

use std::{
    fs::File,
    io::{self, BufWriter, Write},
    thread,
    time::Duration,
};

use crate::{
    cli::{ListOpts, Opts, Package, SubCommand},
    config::Config,
};

mod cli;
mod config;

fn main() -> Result<()> {
    let Opts {
        quiet,
        verbose,
        subcmd,
    } = Opts::parse();

    stderrlog::new()
        .module(module_path!())
        .quiet(quiet)
        .verbosity(verbose)
        .init()?;
    debug!("Subcommand: {:#?}", subcmd);

    debug!("Creating dir structure...");
    dist_utils::path::create_dirs(dist_utils::Mode::Client)?;

    let config = Config::try_new().context("Failed reading config file")?;
    debug!("Config: {:#?}", config);

    match subcmd {
        SubCommand::Sync => {
            // Get the latest package database
            println!("Attempting to sync the latest package database...");
            let response = ureq::get(&format!("{}/packages.db", config.server_url)).call()?;
            let mut db_file = BufWriter::new(File::create(&dist_utils::path::package_db_file())?);
            let mut response_content = response.into_reader();

            println!("Saving the file locally...");
            io::copy(&mut response_content, &mut db_file)?;
            db_file.flush()?;
            println!("Finished syncing");
        }
        SubCommand::Install(Package { name }) => {
            // Get the entry for the package
            let package_db = DistpacDB::connect(
                &dist_utils::path::package_db_file(),
                MissingDBAction::RaiseError,
            )?;
            let entry = package_db
                .query(&name)?
                .ok_or(anyhow::anyhow!("No package entry found for: {}", name))?;

            // Start downloading the package
            println!("Downloading {}...", entry.torrent_name());
            let mut transmission = Transmission::start(
                TransmissionOpts::new().download_dir(dist_utils::path::torrent_data_dir()),
            )?;
            transmission.download_torrent(entry.magnet())?;

            // Wait for the download to be done
            let mut active = false;
            let progress_bar = ProgressBar::new(*entry.size()).with_style(
                ProgressStyle::default_bar()
                    .template("[{wide_bar:.cyan}] {bytes}/{total_bytes} ({bytes_per_sec})")
                    .progress_chars("=> "),
            );
            loop {
                transmission.refresh()?;
                if let Some(torrent) = transmission.get_by_name(entry.torrent_name()) {
                    if torrent.is_finished() {
                        progress_bar.finish_with_message("Finished downloading!");
                        break;
                    }

                    if *torrent.downloaded() != Bytes::zero() {
                        // Just started the actual download so reset to display transfer speed
                        // better
                        if !active {
                            progress_bar.reset();
                            active = true;
                        }
                        progress_bar.set_position(f32::from(*torrent.downloaded()) as u64);
                    }
                }

                thread::sleep(Duration::from_millis(200));
            }

            // FIXME: Permissions aren't set right for torrents so that would need to be fixed
            // // Run the install script for the package
            println!("Installing the package...");
            // let script_location = dist_utils::path::torrent_data_dir()
            //     .join(entry.torrent_name())
            //     .join("scripts")
            //     .join("install.sh");
            // // TODO: handle the command returning an error code
            // Command::new(script_location)
            //     .stdout(Stdio::null())
            //     .stderr(Stdio::null())
            //     .status()?;

            // Finally add the entry to the installed database
            let installed_db = DistpacDB::connect(
                &dist_utils::path::installed_db_file(),
                MissingDBAction::Create,
            )?;
            installed_db.add_package_entry(entry)?;
        }
        SubCommand::Remove(Package { name }) => {
            // TODO: this is done a lot. Would be nice to move it to some common code
            let installed_db = DistpacDB::connect(
                &dist_utils::path::installed_db_file(),
                MissingDBAction::Create,
            )?;
            installed_db.remove_by_name(&name)?;

            // FIXME: Permissions aren't set right for torrents so that would need to be fixed
            // TODO: run the uninstall script
        }
        SubCommand::List(ListOpts { installed }) => {
            // Either reads from the full database or installed database
            let db = if installed {
                DistpacDB::connect(
                    &dist_utils::path::installed_db_file(),
                    MissingDBAction::Create,
                )
            } else {
                DistpacDB::connect(
                    &dist_utils::path::package_db_file(),
                    MissingDBAction::RaiseError,
                )
            }?;
            let packages = db.list_all()?;

            for package in packages {
                display_package(&package);
            }
        }
    }

    Ok(())
}

fn display_package(package: &PackageEntry) {
    println!(
        "{}\t{}\t{}",
        package.name().blue().bold(),
        package.version().to_string().green().bold(),
        pretty_bytes::converter::convert(*package.size() as f64).bold()
    );
}
