use anyhow::Result;
use clap::Clap;
use log::{debug, info};

use crate::{
    cli::{AddPackage, Opts, SubCommand},
    components::ComponentManager,
    packages::add_packages,
};

mod cli;
mod components;
mod config;
mod packages;

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
    debug!("{:#?}", subcmd);

    // Setup all the common directories
    dist_utils::path::create_dirs(dist_utils::Mode::Server)?;

    match subcmd {
        SubCommand::Start(component_listing) => {
            ComponentManager::from(component_listing).start()?;
        }
        SubCommand::Stop(component_listing) => {
            ComponentManager::from(component_listing).stop();
        }
        SubCommand::Add(AddPackage { package_paths }) => {
            info!("Adding packages: {:#?}", package_paths);
            add_packages(package_paths)?;
        }
    }

    Ok(())
}
