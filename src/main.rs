use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::env::args;
use std::time::SystemTime;

mod money;
mod pdf_text;
mod viac_csv;
mod viac_pdf;

use viac_pdf::{ViacDocument, ViacPdf, ViacPdfExtractor, ViacSummary};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let path = args().nth(1).expect("no file given");
    info!("read: {}", path);
    let now = SystemTime::now();

    //let entries = std::fs::read_dir(&path)?
    let entries = walkdir::WalkDir::new(&path).into_iter();
    /*
    .map(|res| res.map(|e| e.path()))
    .collect::<Result<Vec<_>, walkdir::Error>>()?;
    */
    let mut all_docs: HashMap<String, Vec<ViacSummary>> = HashMap::new();
    let pdf_ext = Some(std::ffi::OsStr::new("pdf"));
    for entry in entries
        //.filter_map(|e| Some(e.map(|e| e.path())))
        .filter_map(|e| e.ok())
        .filter(|pfn| pfn.path().extension() == pdf_ext)
    {
        info!("{:?}", entry);
        match ViacPdf::from_path(entry.path()) {
            Ok(vpdf) => {
                let s = match vpdf {
                    ViacPdf::French(p) => {
                        p.print_summary();
                        p.summary()
                    }
                    ViacPdf::German(p) => {
                        p.print_summary();
                        p.summary()
                    }
                };
                match s {
                    Ok(s) => {
                        match s.document_type {
                            ViacDocument::Interest(_) => {
                                debug!("{:?}", s);
                            }
                            ViacDocument::Fees(_) => {
                                debug!("{:?}", s);
                            }
                            ViacDocument::Incoming(_) => {
                                debug!("{:?}", s);
                            }
                            ViacDocument::Dividend(_) => {
                                debug!("{:?}", s);
                            }
                            ViacDocument::TaxReturn(_) => {
                                debug!("{:?}", s);
                            }
                            ViacDocument::FeesRefund(_)
                            | ViacDocument::InterestCharge(_)
                            | ViacDocument::Tax(_)
                            | ViacDocument::TransferIn(_)
                            | ViacDocument::TransferOut(_)
                            | ViacDocument::DeliveryIn(_)
                            | ViacDocument::DeliveryOut(_)
                            | ViacDocument::Outgoing(_) => {
                                unimplemented!();
                            }
                            ViacDocument::Purchase(ref t) | ViacDocument::Sale(ref t) => {
                                debug!("{:?}", s);
                                debug!("Valuta w/o taxes {:?}", &t.valuta_without_taxes());
                                debug!("real shares {:?}", &t.real_shares_count().round_dp(7));
                            }
                            ViacDocument::NotViac => {
                                warn!("PDF author is not Viac");
                                continue;
                            }
                            ViacDocument::Unknown => {
                                warn!("UNKNOWN document_type");
                                continue;
                            }
                        }
                        all_docs
                            .entry(s.portfolio_number.to_string())
                            .or_insert_with(Vec::new)
                            .push(s);
                    }
                    Err(_) => {
                        error!("ERROR pdf unreadable");
                        continue;
                    }
                }
            }
            Err(e) => error!("pdf reading error {e:?}"),
        }
    }
    viac_csv::write_summaries(all_docs)?;

    if let Ok(elapsed) = now.elapsed() {
        info!(
            "Time: {}s",
            elapsed.as_secs() as f64 + elapsed.subsec_nanos() as f64 * 1e-9
        );
    }
    Ok(())
}
