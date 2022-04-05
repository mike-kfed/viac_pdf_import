use std::convert::{AsRef, From};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::{NaiveDate, NaiveDateTime};
use pdf::error::PdfError;
use pdf::file::File;
use rust_decimal::Decimal;

use crate::money::Money;
use crate::pdf_text;

pub enum ViacPdf {
    French(ViacPdfFrench),
    German(ViacPdfGerman),
}

impl ViacPdf {
    pub fn from_path(
        path: impl Into<PathBuf> + AsRef<Path> + AsRef<std::ffi::OsStr>,
    ) -> Result<Self, PdfError> {
        let file = File::<Vec<u8>>::open(&path).unwrap();
        let mut title = None;
        let mut author = None;
        if let Some(ref info) = file.trailer.info_dict {
            title = info.get("Title").map(|p| p.to_string_lossy().unwrap());
            author = info.get("Author").map(|p| p.to_string_lossy().unwrap());
        }
        let pages = pdf_text::pdf2strings(file)?;
        if pages[0].contains("de la Banque WIR") {
            Ok(ViacPdf::French(ViacPdfFrench(ViacPdfData {
                path: PathBuf::from(&path),
                title,
                author,
                pages,
            })))
        } else {
            Ok(ViacPdf::German(ViacPdfGerman(ViacPdfData {
                path: PathBuf::from(&path),
                title,
                author,
                pages,
            })))
        }
    }
}

struct ViacPdfData {
    path: PathBuf,
    title: Option<String>,
    author: Option<String>,
    pages: Vec<String>,
}

pub struct ViacPdfGerman(ViacPdfData);
pub struct ViacPdfFrench(ViacPdfData);

pub trait ViacPdfExtractor {
    fn transaction(&self) -> ViacTransaction {
        ViacTransaction {
            valuta_date: self.valuta_date(),
            shares: self.shares(),
            share_price: self.share_price(),
            total_price: self.total_price(),
            taxes: self.taxes(),
            valuta_price: self.valuta_price(),
            isin: self.isin(),
            share_title: self.share_title(),
            exchange_rate: self.exchange_rate(),
        }
    }

    fn summary(&self) -> Result<ViacSummary, PdfError> {
        let document_type = self.document_type()?;
        let (account_number, portfolio_number) = self.account_numbers();
        Ok(ViacSummary {
            account_number,
            portfolio_number,
            comment: format!("viac_pdf_import {}", self.filename()),
            document_type,
        })
    }

    fn valuta_date(&self) -> NaiveDateTime;
    fn interest_date(&self) -> NaiveDateTime;
    fn shares(&self) -> Decimal;
    fn share_price(&self) -> Money;
    fn total_price(&self) -> Money;
    fn taxes(&self) -> Option<Money>;
    fn valuta_price(&self) -> Money;
    fn isin(&self) -> String;
    fn share_title(&self) -> String;
    fn exchange_rate(&self) -> Option<ExchangeRate>;
    fn document_type(&self) -> Result<ViacDocument, PdfError>;
    fn filename(&self) -> String;
    fn account_numbers(&self) -> (String, String);
    fn exchange_rate_value(&self) -> Decimal;
    fn dividend_price(&self) -> Money;
    fn interest_price(&self) -> Money;
    fn print_summary(&self);
}

impl ViacPdfData {
    pub fn print_summary(&self) {
        println!("author {:?}", self.author);
        println!("title {:?}", self.title);
        self.pages.iter().enumerate().for_each(|(page_nr, text)| {
            println!("=== PAGE {} ===\n", page_nr);
            println!("{}", text);
        });
        println!();
    }

    pub fn filename(&self) -> String {
        self.path
            .clone()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    }

    fn isin(&self) -> String {
        let mut last_line = "";
        for line in self.pages[0].lines() {
            if last_line.starts_with("ISIN:") {
                assert!(!line.is_empty());
                return line.to_string();
            }
            last_line = line;
        }
        unreachable!();
    }
    /// Returns a Money struct from 3line text pattern:
    /// Title
    /// currency
    /// amount
    fn title_currency_amount(&self, title: &str) -> Option<Money> {
        let mut currency = "";
        let mut last_line = "";
        for line in self.pages[0].lines() {
            if last_line.starts_with(title) {
                currency = &line[..3];
                last_line = line;
                continue;
            }
            if !currency.is_empty() {
                // sometimes conversion rate is on an extra line, skip it
                if currency.contains('.') {
                    currency = &line[..3];
                    last_line = line;
                    continue;
                }
                let price = line.replace('\'', "");
                dbg!(&price);
                return Some(Money::new(currency, Decimal::from_str(&price).unwrap()));
            }
            last_line = line;
        }
        None
    }

    fn money_after_line(&self, content: &str) -> Money {
        let mut last_line = "";
        for line in self.pages[0].lines() {
            if last_line == content {
                let price = line[4..].replace('\'', "");
                return Money::new(&line[..3], Decimal::from_str(&price).unwrap());
            }
            last_line = line;
        }
        unreachable!();
    }

    fn account_numbers(&self, account_line: &str, portfolio_line: &str) -> (String, String) {
        let mut last_line = "";
        let mut account_number = String::new();
        let mut portfolio_number = String::new();
        for line in self.pages[0].lines() {
            if last_line == account_line {
                account_number = line.to_string();
            }
            if last_line == portfolio_line {
                portfolio_number = line.to_string();
            }
            if !account_number.is_empty() && !portfolio_number.is_empty() {
                break;
            }
            last_line = line;
        }
        (account_number, portfolio_number)
    }
}

impl ViacPdfExtractor for ViacPdfGerman {
    fn filename(&self) -> String {
        self.0.filename()
    }
    fn print_summary(&self) {
        self.0.print_summary()
    }

    fn document_type(&self) -> Result<ViacDocument, PdfError> {
        if self.0.author != Some("VIAC".to_string()) {
            Ok(ViacDocument::NotViac)
        } else if self.0.pages[0].contains("Börsenabrechnung - Kauf") {
            Ok(ViacDocument::Purchase(self.transaction()))
        } else if self.0.pages[0].contains("Börsenabrechnung - Verkauf") {
            Ok(ViacDocument::Sale(self.transaction()))
        } else if self.0.pages[0].contains("Dividendenausschüttung") {
            let d = ViacDividend {
                isin: self.isin(),
                share_title: self.share_title(),
                valuta_price: self.valuta_price(),
                valuta_date: self.valuta_date(),
                shares: self.shares(),
                dividend_price: self.dividend_price(),
                total_price: self.total_price(),
                exchange_rate: self.exchange_rate(),
            };
            Ok(ViacDocument::Dividend(d))
        } else if self.0.pages[0].contains("Verwaltungsgebühr") {
            let f = ViacValuta {
                valuta_price: self.valuta_price(),
                valuta_date: self.valuta_date(),
            };
            Ok(ViacDocument::Fees(f))
        } else if self.0.pages[0].contains("Zinsgutschrift") {
            let f = ViacValuta {
                valuta_price: self.interest_price(),
                valuta_date: self.interest_date(),
            };
            Ok(ViacDocument::Interest(f))
        } else if self.0.pages[0].contains("Zahlungseingang") {
            let i = ViacValuta {
                valuta_price: self.valuta_price(),
                valuta_date: self.valuta_date(),
            };
            Ok(ViacDocument::Incoming(i))
        } else {
            Ok(ViacDocument::Unknown)
        }
    }

    fn account_numbers(&self) -> (String, String) {
        self.0.account_numbers("Vertrag", "Portfolio")
    }

    fn isin(&self) -> String {
        self.0.isin()
    }

    fn valuta_date(&self) -> NaiveDateTime {
        for line in self.0.pages[0].lines() {
            if line.starts_with("Valuta") {
                return NaiveDate::parse_from_str(line, "Valuta %d.%m.%Y")
                    .unwrap()
                    .and_hms(0, 0, 0);
            }
        }
        unreachable!();
    }

    fn interest_date(&self) -> NaiveDateTime {
        for line in self.0.pages[0].lines() {
            if line.starts_with("Am ") {
                return NaiveDate::parse_from_str(
                    line,
                    "Am %d.%m.%Y haben wir Ihrem Konto gutgeschrieben:",
                )
                .unwrap()
                .and_hms(0, 0, 0);
            }
        }
        unreachable!();
    }

    fn valuta_price(&self) -> Money {
        self.0.title_currency_amount("Valuta").unwrap()
    }

    fn taxes(&self) -> Option<Money> {
        self.0.title_currency_amount("Stempelsteuer")
    }

    fn exchange_rate_value(&self) -> Decimal {
        let mut next_line = false;
        for line in self.0.pages[0].lines() {
            if next_line {
                return Decimal::from_str(line).unwrap();
            }
            if line.starts_with("Umrechnungskurs") {
                if let Some(value) = line.split(' ').nth(2) {
                    if value.is_empty() {
                        next_line = true;
                        continue;
                    }
                    return Decimal::from_str(value).unwrap();
                }
            }
        }
        unreachable!();
    }

    fn exchange_rate(&self) -> Option<ExchangeRate> {
        self.0
            .title_currency_amount("Umrechnungskurs")
            .map(|chf_total| ExchangeRate {
                rate: self.exchange_rate_value(),
                total_price: self.total_price(),
                pdf_price: chf_total,
            })
    }

    fn share_price(&self) -> Money {
        self.0.money_after_line("Kurs:")
    }

    fn dividend_price(&self) -> Money {
        self.0.money_after_line("Ausschüttung:")
    }

    fn total_price(&self) -> Money {
        self.0.title_currency_amount("Betrag").unwrap()
    }

    fn interest_price(&self) -> Money {
        self.0.title_currency_amount("Verrechneter Betrag").unwrap()
    }

    fn shares(&self) -> Decimal {
        let mut last_line = "";
        for line in self.0.pages[0].lines() {
            if line == "Ant" {
                return Decimal::from_str(last_line).unwrap();
            }
            last_line = line;
        }
        unreachable!();
    }

    fn share_title(&self) -> String {
        let mut last_line = "";
        for line in self.0.pages[0].lines() {
            if last_line == "Ant" {
                return line.to_string();
            }
            last_line = line;
        }
        unreachable!();
    }
}

impl ViacPdfExtractor for ViacPdfFrench {
    fn filename(&self) -> String {
        self.0.filename()
    }
    fn print_summary(&self) {
        self.0.print_summary()
    }

    fn document_type(&self) -> Result<ViacDocument, PdfError> {
        if self.0.author != Some("VIAC".to_string()) {
            Ok(ViacDocument::NotViac)
        } else if self.0.pages[0].contains("Opération de bourse - Achat") {
            Ok(ViacDocument::Purchase(self.transaction()))
        } else if self.0.pages[0].contains("Opération de bourse - Vente") {
            Ok(ViacDocument::Sale(self.transaction()))
        } else if self.0.pages[0].contains("Avis de dividende") {
            let d = ViacDividend {
                isin: self.isin(),
                share_title: self.share_title(),
                valuta_price: self.valuta_price(),
                valuta_date: self.valuta_date(),
                shares: self.shares(),
                dividend_price: self.dividend_price(),
                total_price: self.total_price(),
                exchange_rate: self.exchange_rate(),
            };
            Ok(ViacDocument::Dividend(d))
        } else if self.0.pages[0].contains("Commission") {
            let f = ViacValuta {
                valuta_price: self.valuta_price(),
                valuta_date: self.valuta_date(),
            };
            Ok(ViacDocument::Fees(f))
        } else if self.0.pages[0].contains("Intérêts") {
            let f = ViacValuta {
                valuta_price: self.interest_price(),
                valuta_date: self.interest_date(),
            };
            Ok(ViacDocument::Interest(f))
        } else if self.0.pages[0].contains("Avis de versement") {
            let i = ViacValuta {
                valuta_price: self.valuta_price(),
                valuta_date: self.valuta_date(),
            };
            Ok(ViacDocument::Incoming(i))
        } else {
            Ok(ViacDocument::Unknown)
        }
    }

    fn account_numbers(&self) -> (String, String) {
        self.0.account_numbers("Contrat", "Portefeuille")
    }

    fn isin(&self) -> String {
        self.0.isin()
    }

    fn valuta_date(&self) -> NaiveDateTime {
        for line in self.0.pages[0].lines() {
            if line.starts_with("Valeur") {
                return NaiveDate::parse_from_str(line, "Valeur %d.%m.%Y")
                    .unwrap()
                    .and_hms(0, 0, 0);
            }
        }
        unreachable!();
    }

    fn interest_date(&self) -> NaiveDateTime {
        for line in self.0.pages[0].lines() {
            if line.starts_with("Nous avons ") {
                return NaiveDate::parse_from_str(
                    line,
                    "Nous avons crédité le %d.%m.%Y les intérêts suivants:",
                )
                .unwrap()
                .and_hms(0, 0, 0);
            }
        }
        unreachable!();
    }

    fn valuta_price(&self) -> Money {
        self.0.title_currency_amount("Valeur").unwrap()
    }

    fn taxes(&self) -> Option<Money> {
        self.0.title_currency_amount("Droits de timbre")
    }

    fn exchange_rate_value(&self) -> Decimal {
        let mut next_line = false;
        for line in self.0.pages[0].lines() {
            if next_line {
                return Decimal::from_str(line).unwrap();
            }
            if line.starts_with("Taux de conversion") {
                if let Some(value) = line.split(' ').nth(4) {
                    if value.is_empty() {
                        next_line = true;
                        continue;
                    }
                    return Decimal::from_str(value).unwrap();
                }
            }
        }
        unreachable!();
    }

    fn exchange_rate(&self) -> Option<ExchangeRate> {
        self.0
            .title_currency_amount("Taux de conversion")
            .map(|chf_total| ExchangeRate {
                rate: self.exchange_rate_value(),
                total_price: self.total_price(),
                pdf_price: chf_total,
            })
    }

    fn share_price(&self) -> Money {
        self.0.money_after_line("Cours:")
    }

    fn dividend_price(&self) -> Money {
        self.0.money_after_line("Dividende distribué:")
    }

    fn total_price(&self) -> Money {
        self.0.title_currency_amount("Montant").unwrap()
    }

    fn interest_price(&self) -> Money {
        self.0.title_currency_amount("Montant crédité").unwrap()
    }

    fn shares(&self) -> Decimal {
        let mut last_line = "";
        let mut two_lines = "";
        for line in self.0.pages[0].lines() {
            if line.starts_with("ISIN:") {
                return Decimal::from_str(two_lines).unwrap();
            }
            two_lines = last_line;
            last_line = line;
        }
        unreachable!();
    }

    fn share_title(&self) -> String {
        let mut last_line = "";
        for line in self.0.pages[0].lines() {
            if line.starts_with("ISIN:") {
                return last_line.to_string();
            }
            last_line = line;
        }
        unreachable!();
    }
}

#[derive(Debug)]
pub enum ViacDocument {
    Unknown,
    NotViac,
    Purchase(ViacTransaction),
    Sale(ViacTransaction),
    Dividend(ViacDividend),
    Fees(ViacValuta),
    Interest(ViacValuta),
    Incoming(ViacValuta),
}

#[derive(Debug)]
pub struct ViacDividend {
    isin: String,
    share_title: String,
    valuta_date: NaiveDateTime,
    valuta_price: Money,
    shares: Decimal,
    dividend_price: Money,
    total_price: Money,
    exchange_rate: Option<ExchangeRate>,
}

impl ViacDividend {
    pub fn real_shares_count(&self) -> Decimal {
        assert_eq!(self.total_price.currency, self.dividend_price.currency);
        // TODO instead of log to stdout, write to comment of transaction
        // TODO use real_shares_count calc from ViacTransaction
        println!(
            "dividend computed_count: {} pdf_count:{}",
            (self.total_price.amount / self.dividend_price.amount).round_dp(5),
            self.shares
        );
        self.total_price.amount / self.dividend_price.amount
    }
}

#[derive(Debug)]
pub struct ViacValuta {
    valuta_date: NaiveDateTime,
    valuta_price: Money,
}

#[derive(Debug)]
pub struct ViacSummary {
    pub account_number: String,
    pub portfolio_number: String,
    pub comment: String,
    pub document_type: ViacDocument,
}

impl ViacSummary {
    pub fn valuta_date(&self) -> NaiveDateTime {
        match &self.document_type {
            ViacDocument::Interest(s) | ViacDocument::Fees(s) | ViacDocument::Incoming(s) => {
                s.valuta_date
            }
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s.valuta_date,
            ViacDocument::Dividend(s) => s.valuta_date,
            _ => unreachable!(),
        }
    }

    pub fn order_type(&self) -> String {
        match &self.document_type {
            ViacDocument::Interest(_) => "Zinsen",
            ViacDocument::Fees(_) => "Gebühren",
            ViacDocument::Incoming(_) => "Einlage",
            ViacDocument::Purchase(_) => "Kauf",
            ViacDocument::Sale(_) => "Verkauf",
            ViacDocument::Dividend(_) => "Dividende",
            _ => unreachable!(),
        }
        .to_string()
    }

    pub fn valuta_price(&self) -> (String, String) {
        let v = match &self.document_type {
            ViacDocument::Interest(s) | ViacDocument::Fees(s) | ViacDocument::Incoming(s) => {
                s.valuta_price
            }
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s.valuta_price,
            ViacDocument::Dividend(s) => s.valuta_price,
            _ => unreachable!(),
        };
        (
            v.amount.to_string(),
            std::str::from_utf8(&v.currency).unwrap().to_string(),
        )
    }

    pub fn total_price(&self) -> (String, String) {
        match &self.document_type {
            ViacDocument::Interest(_) | ViacDocument::Fees(_) | ViacDocument::Incoming(_) => {
                ("".to_owned(), "".to_owned())
            }
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => (
                s.total_price.amount.to_string(),
                std::str::from_utf8(&s.total_price.currency)
                    .unwrap()
                    .to_string(),
            ),
            ViacDocument::Dividend(s) => (
                s.total_price.amount.to_string(),
                std::str::from_utf8(&s.total_price.currency)
                    .unwrap()
                    .to_string(),
            ),
            _ => unreachable!(),
        }
    }

    pub fn exchange_rate(&self) -> String {
        match &self.document_type {
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s
                .exchange_rate
                .as_ref()
                .map_or("".to_owned(), |x| x.rate.to_string()),
            ViacDocument::Dividend(s) => s
                .exchange_rate
                .as_ref()
                .map_or("".to_owned(), |x| x.rate.to_string()),
            _ => "".to_owned(),
        }
    }
    pub fn fees(&self) -> String {
        match &self.document_type {
            ViacDocument::Fees(s) => s.valuta_price.amount.to_string(),
            _ => "0.00".to_string(),
        }
    }
    pub fn taxes(&self) -> String {
        match &self.document_type {
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s
                .taxes
                .as_ref()
                .map_or("0.00".to_string(), |t| t.amount.to_string()),
            _ => "0.00".to_string(),
        }
    }
    pub fn shares(&self) -> String {
        match &self.document_type {
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => {
                s.real_shares_count().round_dp(5).to_string()
            }
            ViacDocument::Dividend(s) => s.real_shares_count().round_dp(5).to_string(),
            _ => "0.00".to_string(),
        }
    }
    pub fn isin(&self) -> String {
        match &self.document_type {
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s.isin.to_owned(),
            ViacDocument::Dividend(s) => s.isin.to_owned(),
            _ => "".to_string(),
        }
    }
    pub fn share_title(&self) -> String {
        match &self.document_type {
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s.share_title.to_owned(),
            ViacDocument::Dividend(s) => s.share_title.to_owned(),
            _ => "".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct ViacTransaction {
    valuta_date: NaiveDateTime,
    shares: Decimal,
    share_price: Money,
    total_price: Money,
    valuta_price: Money,
    taxes: Option<Money>,
    isin: String,
    share_title: String,
    exchange_rate: Option<ExchangeRate>,
}

#[derive(Debug)]
pub struct ExchangeRate {
    rate: Decimal,
    total_price: Money,
    pub pdf_price: Money,
}

impl ExchangeRate {
    /// If exchange_rate is given we can use it compute a total_price with more decimal digits
    pub fn total_price_chf(&self) -> Money {
        assert_ne!(self.total_price.currency, crate::money::CHF);
        Money::new("CHF", self.total_price.amount * self.rate)
    }
}

impl ViacTransaction {
    pub fn valuta_without_taxes(&self) -> Money {
        match &self.taxes {
            Some(taxes) => {
                assert_eq!(self.valuta_price.currency, taxes.currency);
                Money::new("CHF", self.valuta_price.amount - taxes.amount)
            }
            None => self.valuta_price,
        }
    }

    /// only corrects shares amount found if the share-price diverges by more than 1%
    pub fn real_shares_count(&self) -> Decimal {
        // start with higher precision total_price if exchange-rate is given
        let (total_price, share_price) = match &self.exchange_rate {
            Some(er) => (
                er.total_price_chf(),
                Money::new("CHF", self.share_price.amount * er.rate),
            ),
            None => (self.total_price, self.share_price),
        };
        assert_eq!(total_price.currency, share_price.currency);
        let pp_share_price = total_price.amount / self.shares;
        let real_count = total_price.amount / share_price.amount;
        //let share_price_diff = (pp_share_price - self.share_price.amount).abs();
        let share_price_diff = ((Decimal::ONE - (pp_share_price / share_price.amount).abs())
            * Decimal::ONE_HUNDRED)
            .round_dp(4);
        if share_price_diff > Decimal::ONE {
            // TODO instead of log to stdout, write to comment of transaction
            println!(
                "share_price_diff: {}% computed_count: {} pdf_count:{}",
                share_price_diff,
                real_count.round_dp(5),
                self.shares
            );
            real_count
        } else {
            self.shares
        }
    }
}
