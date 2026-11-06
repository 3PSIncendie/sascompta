//! Alias binary for `demo_export_tax` (same behavior).
use sascompta_lib::db;
use std::env;
use std::path::PathBuf;

fn main() {
    let mut args = env::args().skip(1);
    let workspace = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/sascompta-demo.sascompta"));
    let out_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    if !workspace.exists() {
        if let Err(e) = db::create_demo_workspace(&workspace) {
            eprintln!("Création démo échouée : {e}");
            std::process::exit(1);
        }
    }

    let conn = match db::open_connection(&workspace) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Ouverture échouée : {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("out_dir : {e}");
        std::process::exit(1);
    }

    let vat = match db::tax::vat_report(&conn, "2026-01-01", "2026-12-31") {
        Ok(r) => r,
        Err(e) => {
            eprintln!("TVA : {e}");
            std::process::exit(1);
        }
    };
    let aux = match db::tax::auxiliary_balance(&conn, Some("2026-01-01"), Some("2026-12-31")) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Aux : {e}");
            std::process::exit(1);
        }
    };

    let vat_path = out_dir.join("tva-ca3-demo.csv");
    let aux_path = out_dir.join("balance-auxiliaire-demo.csv");
    if let Err(e) = std::fs::write(&vat_path, db::tax::export_vat_csv(&vat)) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    if let Err(e) = std::fs::write(&aux_path, db::tax::export_aux_csv(&aux)) {
        eprintln!("{e}");
        std::process::exit(1);
    }

    println!("Export TVA  → {}", vat_path.display());
    println!(
        "  collectée={} déductible={} net={}",
        vat.collected_cents, vat.deductible_cents, vat.net_cents
    );
    println!("Export aux → {}", aux_path.display());
}
