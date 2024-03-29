use std::convert::{AsRef, From};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::{NaiveDate, NaiveDateTime};
use log::debug;
use pdf::error::PdfError;
use pdf::file::FileOptions;
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
        let file = FileOptions::cached().open(&path).unwrap();
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

    fn dividend(&self) -> ViacDividend {
        ViacDividend {
            isin: self.isin(),
            share_title: self.share_title(),
            valuta_price: self.valuta_price(),
            valuta_date: self.valuta_date(),
            shares: self.shares(),
            dividend_price: self.dividend_price(),
            total_price: self.total_price(),
            exchange_rate: self.exchange_rate(),
        }
    }

    fn summary(&self, deduce: bool) -> Result<ViacSummary, PdfError> {
        let document_type = self.document_type()?;
        let (account_number, portfolio_number) = self.account_numbers();
        Ok(ViacSummary {
            deduce,
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
        debug!("author {:?}", self.author);
        debug!("title {:?}", self.title);
        self.pages.iter().enumerate().for_each(|(page_nr, text)| {
            debug!("=== PAGE {} ===\n", page_nr);
            debug!("{}", text);
        });
        debug!("// --");
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
            if self.0.pages[0].contains("Rückerstattung Quellensteuer") {
                Ok(ViacDocument::TaxReturn(self.dividend()))
            } else if self.0.pages[0].contains("Korrektur Dividendenausschüttung") {
                Ok(ViacDocument::Unknown) // TODO: treat storno of dividends
            } else {
                Ok(ViacDocument::Dividend(self.dividend()))
            }
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
        } else if self.0.pages[0].contains("____impossible_____FeesRefund") {
            Ok(ViacDocument::FeesRefund(0))
        } else if self.0.pages[0].contains("____impossible_____InterestCharge") {
            Ok(ViacDocument::InterestCharge(0))
        } else if self.0.pages[0].contains("____impossible_____Outgoing") {
            Ok(ViacDocument::Outgoing(0))
        } else if self.0.pages[0].contains("____impossible_____Tax") {
            Ok(ViacDocument::Tax(0))
        } else if self.0.pages[0].contains("____impossible_____TransferIn") {
            Ok(ViacDocument::TransferIn(0))
        } else if self.0.pages[0].contains("____impossible_____TransferOut") {
            Ok(ViacDocument::TransferOut(0))
        } else if self.0.pages[0].contains("____impossible_____DeliveryIn") {
            Ok(ViacDocument::DeliveryIn(0))
        } else if self.0.pages[0].contains("____impossible_____DeliveryOut") {
            Ok(ViacDocument::DeliveryOut(0))
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
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
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
                .and_hms_opt(0, 0, 0)
                .unwrap();
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
            if self.0.pages[0].contains("Remboursement d'impôt à la source") {
                Ok(ViacDocument::TaxReturn(self.dividend()))
            } else {
                Ok(ViacDocument::Dividend(self.dividend()))
            }
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
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
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
                .and_hms_opt(0, 0, 0)
                .unwrap();
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
    FeesRefund(i32),
    Interest(ViacValuta),
    InterestCharge(i32),
    Incoming(ViacValuta),
    Outgoing(i32),
    Tax(i32),
    TaxReturn(ViacDividend),
    TransferIn(i32),
    TransferOut(i32),
    DeliveryIn(i32),
    DeliveryOut(i32),
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
        debug!(
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
    deduce: bool,
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
            ViacDocument::Dividend(s) | ViacDocument::TaxReturn(s) => s.valuta_date,
            _ => unreachable!(),
        }
    }

    /// from name.abuchen.portfolio.model.AccountTransaction enum Type
    pub fn order_type(&self) -> String {
        match &self.document_type {
            ViacDocument::Interest(_) => "INTEREST", // "Zinsen",
            ViacDocument::InterestCharge(_) => "INTEREST_CHARGE", // "Zinsbelastung"
            ViacDocument::Fees(_) => "FEES",         //"Gebühren",
            ViacDocument::FeesRefund(_) => "FEES_REFUND", //"Gebührenrückerstattung",
            ViacDocument::Incoming(_) => "DEPOSIT",  //"Einlage",
            ViacDocument::Outgoing(_) => "REMOVAL",  //"Entnahme",
            ViacDocument::Purchase(_) => "BUY",      //"Kauf",
            ViacDocument::Sale(_) => "SELL",         // "Verkauf",
            ViacDocument::Dividend(_) => "DIVIDENDS", //"Dividende",
            ViacDocument::TaxReturn(_) => "TAX_REFUND", //"Steuerrückerstattung",
            ViacDocument::Tax(_) => "TAXES",         //"Steuern",
            ViacDocument::TransferIn(_) => "TRANSFER_IN", //"Umbuchung (Eingang)",
            ViacDocument::TransferOut(_) => "TRANSFER_OUT", //"Umbuchung (Ausgang)",
            ViacDocument::DeliveryIn(_) => "DELIVERY_INBOUND", //"Einlieferung",
            ViacDocument::DeliveryOut(_) => "DELIVERY_OUTBOUND", //"Auslieferung",
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
            ViacDocument::Dividend(s) | ViacDocument::TaxReturn(s) => s.valuta_price,
            _ => unreachable!(),
        };
        (
            v.amount.to_string(),
            std::str::from_utf8(&v.currency).unwrap().to_string(),
        )
    }

    pub fn total_price(&self, conversion_rate: Decimal) -> (String, String) {
        match &self.document_type {
            ViacDocument::Interest(_) | ViacDocument::Fees(_) | ViacDocument::Incoming(_) => {
                ("".to_owned(), "".to_owned())
            }
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => (
                (s.total_price.amount * conversion_rate).to_string(),
                std::str::from_utf8(&s.total_price.currency)
                    .unwrap()
                    .to_string(),
            ),
            ViacDocument::Dividend(s) | ViacDocument::TaxReturn(s) => (
                (s.total_price.amount * conversion_rate).to_string(),
                std::str::from_utf8(&s.total_price.currency)
                    .unwrap()
                    .to_string(),
            ),
            _ => unreachable!(),
        }
    }

    /// VIAC documents are rounded to 2 decimals, exchange rate is therefore not making PP happy, compute it
    pub fn exchange_rate_compute(&self, conversion_rate: Decimal) -> String {
        let v = match &self.document_type {
            ViacDocument::Interest(s) | ViacDocument::Fees(s) | ViacDocument::Incoming(s) => {
                s.valuta_price
            }
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s.valuta_price,
            ViacDocument::Dividend(s) | ViacDocument::TaxReturn(s) => s.valuta_price,
            _ => unreachable!(),
        };
        let t = match &self.document_type {
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s.total_price,
            ViacDocument::Dividend(s) | ViacDocument::TaxReturn(s) => s.total_price,
            _ => unreachable!(),
        };
        (v.amount / t.amount * conversion_rate)
            .round_dp(5)
            .to_string()
    }

    pub fn exchange_rate(&self, conversion_rate: Decimal) -> String {
        match &self.document_type {
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s
                .exchange_rate
                .as_ref()
                .map_or("".to_owned(), |x| (x.rate * conversion_rate).to_string()),
            ViacDocument::Dividend(s) | ViacDocument::TaxReturn(s) => s
                .exchange_rate
                .as_ref()
                .map_or("".to_owned(), |x| (x.rate * conversion_rate).to_string()),
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
                if self.deduce {
                    s.real_shares_count().round_dp(5).to_string()
                } else {
                    s.shares.to_string()
                }
            }
            ViacDocument::Dividend(s) | ViacDocument::TaxReturn(s) => {
                if self.deduce {
                    s.real_shares_count().round_dp(5).to_string()
                } else {
                    s.shares.to_string()
                }
            }
            _ => "0.00".to_string(),
        }
    }
    pub fn isin(&self) -> String {
        match &self.document_type {
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s.isin.to_owned(),
            ViacDocument::Dividend(s) | ViacDocument::TaxReturn(s) => s.isin.to_owned(),
            _ => "".to_string(),
        }
    }
    pub fn share_title(&self) -> String {
        match &self.document_type {
            ViacDocument::Purchase(s) | ViacDocument::Sale(s) => s.share_title.to_owned(),
            ViacDocument::Dividend(s) | ViacDocument::TaxReturn(s) => s.share_title.to_owned(),
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
        let share_price_diff = ((Decimal::ONE - (pp_share_price / share_price.amount).abs())
            * Decimal::ONE_HUNDRED)
            .round_dp(4);
        if share_price_diff > Decimal::ONE {
            // TODO not just log, also write to comment of transaction
            debug!(
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
