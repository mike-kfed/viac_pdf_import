# VIAC PDF import for Portfolio Performance

Idea, correctly import from VIAC PDFs by creating multiple CSV files for import into PP.
Avoiding the calculation error for the share-price making buy/sale look wrong on plots, see issue buchen/portfolio#2445

## problems

- the fake higher precision share-count number helps with the plots, but it of course fails to be correct.
  Meaning if VIAC for example sells all shares of a fund you many end up with a negative fraction of a share, which PP catches as a consistency error. Less obvious but still wrong is a super small positive fraction of the Fund still being held after. No real solution for that.
- not yet conveniently usable with an existing PP database

## features

- supports german and french VIAC pdf files
- changes shares amount to better match the actual share share-price
- separate export of all securities found in the PDFs
- separate export of account transactions
- separate export of portfolio transactions
- does all math using Decimal rounded to 5 digit precision
- recursively opens all PDF found in input-directory

## output

- per VIAC Portfolio
  1. CSV with shares buy/sell
  2. CSV with Einlage, Dividende, Steurrückerstattung, Gebühren, Zinsen
  3. CSV with all Shares and their currencies

## howto import

1. import the 3 json files from the `PP-import` folder
2. import all the CSV files ending with `_Shares.csv`
3. import all the CSV files ending with `_Konto.csv`
4. import all the CSV files ending with `_Transactions.csv`

## install and run

1. follow Rust install instructions of https://rustup.rs/
2. clone this repo
3. `cd viac_pdf_importer`
4. `cargo run --release -- <DIR_WITH_ALL_VIAC_PDF>`
