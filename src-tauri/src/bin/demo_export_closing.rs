//! Export pack ZIP + FEC brouillon pour artifacts J7 (optionnellement clôture).

use sascompta_lib::db::{self, closing, statements};
use std::env;
use std::path::PathBuf;

fn main() {
    let workspace = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/sascompta-j7-demo.sascompta"));
    let out = env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/sascompta-j7-export"));
    let do_close = env::args().any(|a| a == "--close");

    if workspace.exists() {
        let _ = std::fs::remove_file(&workspace);
    }
    let info = db::create_demo_workspace(&workspace).expect("seed");
    let (conn, _) =
        db::open_workspace(&workspace.display().to_string(), db::DEMO_PASSWORD).expect("open");

    std::fs::create_dir_all(&out).unwrap();
    let fy = closing::current_fiscal_year(&conn).expect("fy");
    let filter = statements::year_filter(&fy.start_date, &fy.end_date);

    let fec_path = out.join(format!("FEC-brouillon-{}.txt", fy.label));
    closing::write_fec_file(&conn, &filter, &fec_path).expect("fec");

    let zip_path = out.join(format!("pack-comptes-annuels-{}.zip", fy.label));
    let meta = closing::export_publication_zip(&conn, &fy, &zip_path).expect("zip");

    let checklist = closing::closing_checklist(&conn).expect("checklist");
    let checklist_json = serde_json::to_string_pretty(&checklist).unwrap();
    std::fs::write(out.join("checklist-cloture.json"), checklist_json).unwrap();

    println!(
        "OK {} — résultat {} — FEC {} — ZIP {} ({} octets)",
        info.organization.name,
        meta.resultat_cents,
        fec_path.display(),
        zip_path.display(),
        std::fs::metadata(&zip_path).unwrap().len()
    );

    if do_close {
        let res = closing::close_fiscal_year(&conn).expect("close");
        println!(
            "CLOTURE {} → {} AN={}",
            res.closed_year.label,
            res.next_year.label,
            res.an_ecriture_num.as_deref().unwrap_or("")
        );
        let closed_json = serde_json::to_string_pretty(&res).unwrap();
        std::fs::write(out.join("close-result.json"), closed_json).unwrap();
    }
}
