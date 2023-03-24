//! command line options
use std::{path::PathBuf, str::FromStr};
use thiserror::Error;

#[derive(clap::Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub(crate) struct Cli {
    /// Directory where VIAC pdfs will be recursively looked for
    #[clap(short, long)]
    pub directory: PathBuf,
    /// try to deduce the correct amount of stocks bought/sold
    /// attention this can create different problems
    #[clap(short = 'A', long)]
    pub deduce_amount: bool,
    /// help convert the currency to the one PP expects
    /// useful for when the Funds online information is in a difference currency than the trade that happens
    /// format: AT3456789014,USD
    #[clap(short, long)]
    pub isin_currency: Vec<IsinCurrency>,
}

#[derive(Clone)]
pub struct IsinCurrency {
    pub isin: isin::ISIN,
    pub currency: [u8; 3],
}

impl FromStr for IsinCurrency {
    type Err = IsinCurrencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((isin, currency)) = s.split_once(',') {
            Ok(Self {
                isin: isin.parse().map_err(Self::Err::IsinError)?,
                currency: currency
                    .as_bytes()
                    .try_into()
                    .map_err(|_| Self::Err::CurrencyNotThreeChar)?,
            })
        } else {
            Err(Self::Err::IsinAndCurrencyNotFound)
        }
    }
}

impl std::fmt::Debug for IsinCurrency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IsinCurrency")
            .field(
                "currency",
                &String::from_utf8(self.currency.to_vec()).unwrap(),
            )
            .field("isin", &self.isin)
            .finish()
    }
}

impl std::fmt::Display for IsinCurrency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}",
            std::str::from_utf8(&self.currency).unwrap(),
            self.isin
        )
    }
}

#[derive(Debug, Error)]
pub enum IsinCurrencyError {
    #[error("ISIN parser failed: {0}")]
    IsinError(isin::ISINError),
    #[error("currency code must be 3 chars long")]
    CurrencyNotThreeChar,
    #[error("comma separator not found")]
    IsinAndCurrencyNotFound,
}
