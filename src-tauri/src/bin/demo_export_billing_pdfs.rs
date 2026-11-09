//! Export PDF devis / facture démo pour artifacts UI.

use sascompta_lib::db::{self, billing};
use std::env;
use std::path::PathBuf;

fn main() {
    let workspace = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/sascompta-billing-demo.sascompta"));
    let out = env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/sascompta-billing-pdfs"));

    if workspace.exists() {
        let _ = std::fs::remove_file(&workspace);
    }
    std::fs::create_dir_all(&out).expect("mkdir out");

    db::create_demo_workspace(&workspace).expect("seed demo");
    let (conn, info) =
        db::open_workspace(&workspace.display().to_string(), db::DEMO_PASSWORD).expect("open");

    let quote = billing::list_quotes(&conn)
        .expect("quotes")
        .into_iter()
        .next()
        .expect("demo quote");
    let quote_pdf = out.join("apercu-pdf-devis.pdf");
    billing::export_quote_pdf(&conn, quote.id, &quote_pdf).expect("quote pdf");

    let draft = billing::list_invoices(&conn)
        .expect("invoices")
        .into_iter()
        .find(|i| i.status == "draft")
        .expect("demo invoice draft");
    let issued = billing::issue_invoice(&conn, draft.id).expect("issue");
    let invoice_pdf = out.join("apercu-pdf-facture.pdf");
    billing::export_invoice_pdf(&conn, issued.id, &invoice_pdf).expect("invoice pdf");

    println!(
        "OK {} — devis {} → {} | facture {} → {}",
        info.organization.name,
        quote.number,
        quote_pdf.display(),
        issued.number.as_deref().unwrap_or("?"),
        invoice_pdf.display()
    );
}
