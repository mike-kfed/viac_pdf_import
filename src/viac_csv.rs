use crate::viac_pdf::{ViacDocument, ViacSummary};
use std::collections::HashMap;

struct ShareInfo {
    isin: String,
    name: String,
    currency: String,
    comment: String,
}

pub fn write_summaries(viac_summaries: HashMap<String, Vec<ViacSummary>>) -> std::io::Result<()> {
    // first write out all shares
    let mut all_shares: HashMap<String, ShareInfo> = HashMap::new();
    let mut file = std::fs::File::create("VIAC_any_account_Shares.csv")?;
    let mut wtr = csv::Writer::from_writer(&mut file);
    wtr.write_record(&[
        "ISIN",
        "WKN",
        "Ticker-Symbol",
        "Wertpapiername",
        "Währung",
        "Notiz",
    ])?;
    for (_portfolio, summaries) in viac_summaries.iter() {
        // VIAC sometimes buys in currency X and delivers dividends in currency Y
        // we only consider transactions to determine the shares "currency"
        // later we fake the currency for dividends
        summaries
            .iter()
            .filter(|s| {
                matches!(
                    s.document_type,
                    ViacDocument::Purchase(_) | ViacDocument::Sale(_)
                )
            })
            .for_each(|s| {
                let isin = s.isin();
                if !isin.is_empty() {
                    let (_, currency) = s.total_price();
                    all_shares.entry(s.isin()).or_insert_with(|| ShareInfo {
                        isin,
                        name: s.share_title(),
                        currency,
                        comment: "viac_pdf_import".to_string(),
                    });
                }
            });
    }
    for v in all_shares.values() {
        wtr.write_record(&[
            v.isin.to_string(),     // "ISIN",
            "".to_string(),         // "WKN",
            "".to_string(),         //"Ticker-Symbol",
            v.name.to_string(),     //"Wertpapiername",
            v.currency.to_string(), //"Währung",
            v.comment.to_string(),  // "Notiz",
        ])?;
    }
    // now all transactions
    let header = &[
        "Datum",
        "Typ",
        "Wert",
        "Buchungswährung",
        "Bruttobetrag",
        "Währung Bruttobetrag",
        "Wechselkurs",
        "Gebühren",
        "Steuern",
        "Stück",
        "ISIN",
        "Notiz",
    ];
    for (portfolio, mut summaries) in viac_summaries.into_iter() {
        summaries.sort_by_key(|s| s.valuta_date());
        let mut file = std::fs::File::create(&format!("VIAC_{}_Account.csv", portfolio))?;
        let mut wtr = csv::Writer::from_writer(&mut file);
        wtr.write_record(header)?;
        summaries
            .iter()
            .filter(|s| {
                !matches!(
                    s.document_type,
                    ViacDocument::Purchase(_) | ViacDocument::Sale(_)
                )
            })
            .for_each(|summary| {
                let (valuta_price, valuta_currency) = summary.valuta_price();
                let (total_price, mut total_currency) = summary.total_price();
                let isin = summary.isin();
                let exchange_rate;
                if !isin.is_empty() {
                    if let Some(share) = all_shares.get(&isin) {
                        let share_currency = &share.currency;
                        // fake exchange-rate of 1.0 when dividend is not paid in share-currency
                        if share_currency != &total_currency {
                            total_currency = share_currency.to_owned();
                            exchange_rate = "1.00".to_string();
                        } else {
                            exchange_rate = summary.exchange_rate();
                        }
                    } else {
                        panic!("Share {isin} not found, make sure to import all PDFs");
                    }
                } else {
                    exchange_rate = summary.exchange_rate();
                }

                wtr.write_record(&[
                    summary.valuta_date().to_string(), //"Datum",
                    summary.order_type(),              //"Typ",
                    valuta_price,                      //"Wert",
                    valuta_currency,                   //"Buchungswährung",
                    total_price,                       //"Bruttobetrag",
                    total_currency,                    //"Währung Bruttobetrag",
                    exchange_rate,                     //"Wechselkurs",
                    summary.fees(),                    //"Gebühren"
                    summary.taxes(),                   //"Steuern"
                    summary.shares(),                  //"Stück"
                    isin,                              //"ISIN"
                    summary.comment.to_owned(),
                ])
                .unwrap();
            });
        let mut file = std::fs::File::create(&format!("VIAC_{}_Portfolio.csv", portfolio))?;
        let mut wtr = csv::Writer::from_writer(&mut file);
        wtr.write_record(header)?;
        summaries
            .iter()
            .filter(|s| {
                matches!(
                    s.document_type,
                    ViacDocument::Purchase(_) | ViacDocument::Sale(_)
                )
            })
            .for_each(|summary| {
                let (valuta_price, valuta_currency) = summary.valuta_price();
                let (total_price, total_currency) = summary.total_price();
                let isin = summary.isin();
                // TODO: track all shares-count, if at the end close to zero

                wtr.write_record(&[
                    summary.valuta_date().to_string(), //"Datum",
                    summary.order_type(),              //"Typ",
                    valuta_price,                      //"Wert",
                    valuta_currency,                   //"Buchungswährung",
                    total_price,                       //"Bruttobetrag",
                    total_currency,                    //"Währung Bruttobetrag",
                    summary.exchange_rate(),           //"Wechselkurs",
                    summary.fees(),                    //"Gebühren"
                    summary.taxes(),                   //"Steuern"
                    summary.shares(),                  //"Stück"
                    isin,                              //"ISIN"
                    summary.comment.to_owned(),
                ])
                .unwrap();
            });
    }
    Ok(())
}
