//! command line options
use std::path::PathBuf;

#[derive(clap::Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub(crate) struct Cli {
    /// Directory where VIAC pdfs will be recursively looked for
    #[clap(short, long)]
    pub directory: PathBuf,
    /// try to deduce the correct amount of stocks bought/sold
    /// attention this can create different problems
    #[clap(short, long)]
    pub deduce_amount: bool,
    /// help convert the currency to the one PP expects
    /// useful for when the Funds online information is in a difference currency than the trade that happens
    #[clap(short, long)]
    pub isin_currency: Vec<Option<String>>,
}
