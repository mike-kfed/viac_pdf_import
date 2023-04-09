# VIAC PDF import for Portfolio Performance

Idea, correctly import from VIAC PDFs by creating multiple CSV files for import into PP.
Avoiding the calculation error for the share-price making buy/sale look wrong on plots, see issue [buchen/portfolio#2445](https://github.com/buchen/portfolio/issues/2545)

## problems

- the fake higher precision share-count number helps with the plots, but it of course fails to be correct.
  Meaning if VIAC for example sells all shares of a fund you many end up with a negative fraction of a share, which PP catches as a consistency error. Less obvious but still wrong is a super small positive fraction of the Fund still being held after. No real solution for that.
- not yet conveniently usable with an existing PP database

## features

- supports german and french VIAC pdf files
- optionally changes shares amount to better match the actual share-price
- separate export of all securities found in the PDFs
- separate export of account transactions
- separate export of portfolio transactions
- does all math using Decimal rounded to 5 digit precision
- recursively opens all PDF found in input-directory
- control output using `RUST_LOG` environment variable
- optionally converts the ISIN currency to what Portfolio Performance needs.

## output

- per VIAC Portfolio
  1. CSV with shares buy/sell
  2. CSV with Einlage, Dividende, Steurrückerstattung, Gebühren, Zinsen
  3. CSV with all Shares and their currencies

## howto import

1. import the 3 JSON files from the `PP-import` folder, they are the configurations for CSV importing
2. import the CSV file called `VIAC_any_account_Shares.csv` with the "VIAC CSV Import Shares" config
3. import all the CSV files ending with `_Account.csv` with the "VIAC CSV Import Account" config
4. import all the CSV files ending with `_Portfolio.csv` with the "VIAC CSV Import Portfolio" config

## install and run

1. follow Rust install instructions of https://rustup.rs/
2. clone this repo
3. `cd viac_pdf_importer`
4. `RUST_LOG=info cargo run --release -- -d <DIR_WITH_ALL_VIAC_PDF>`

## historical exchange rates

can be found here <https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html>
