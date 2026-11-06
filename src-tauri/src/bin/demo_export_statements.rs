//! Export états annuels démo (bilan / CR / annexe / PDF) pour artifacts J6.

use sascompta_lib::db::{self, statements};
use std::env;
use std::path::PathBuf;

fn main() {
    let workspace = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/sascompta-j6-demo.sascompta"));
    let out = env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/sascompta-j6-export"));

    if workspace.exists() {
        let _ = std::fs::remove_file(&workspace);
    }
    let info = db::create_demo_workspace(&workspace).expect("seed demo");
    let (conn, _) =
        db::open_workspace(&workspace.display().to_string(), db::DEMO_PASSWORD).expect("open");

    let filter = sascompta_lib::db::statements::year_filter(
        &info.fiscal_year.start_date,
        &info.fiscal_year.end_date,
    );
    let pack = statements::build_annual_pack(&conn, &filter).expect("pack");
    statements::export_pack_files(&out, &pack).expect("export");

    println!(
        "OK {} — actif {} passif {} résultat {} → {}",
        info.organization.name,
        pack.bilan.total_actif_cents,
        pack.bilan.total_passif_cents,
        pack.compte_resultat.resultat_cents,
        out.display()
    );
}
