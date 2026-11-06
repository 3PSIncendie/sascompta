use sascompta_lib::db::{self, assets, open_connection, open_workspace};
use std::path::PathBuf;

fn main() {
    let workspace = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/sascompta-demo/demo.sascompta"));
    let out_dir = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/sascompta-j5-export"));
    let _ = std::fs::create_dir_all(&out_dir);
    if !workspace.exists() {
        db::create_demo_workspace(&workspace).expect("seed");
    }
    let (conn, _) = open_workspace(&workspace.display().to_string(), db::DEMO_PASSWORD).expect("open");
    let assets_list = assets::list_assets(&conn).expect("assets");
    let mut reg = String::from("Code;Libellé;Compte;Base;Résidu;Durée;Dotation annuelle;Mise en service\n");
    for a in &assets_list {
        reg.push_str(&format!(
            "{};{};{};{},{:02};{},{:02};{};{},{:02};{}\n",
            a.code, a.label, a.account_number,
            a.base_cents / 100, (a.base_cents % 100).abs(),
            a.residual_cents / 100, (a.residual_cents % 100).abs(),
            a.useful_life_years,
            a.annual_dotation_cents / 100, (a.annual_dotation_cents % 100).abs(),
            a.service_date
        ));
    }
    let reg_path = out_dir.join("registre-immos-demo.csv");
    std::fs::write(&reg_path, reg).unwrap();

    let asset_id = assets_list[0].id;
    let plan = assets::asset_schedule(&conn, asset_id).unwrap();
    let mut sch = String::from("Année;Période;Dotation;Cumul;VNC\n");
    for r in plan {
        sch.push_str(&format!(
            "{};{};{},{:02};{},{:02};{},{:02}\n",
            r.year_index, r.period_label,
            r.amount_cents / 100, (r.amount_cents % 100).abs(),
            r.cumulative_cents / 100, (r.cumulative_cents % 100).abs(),
            r.net_book_value_cents / 100, (r.net_book_value_cents % 100).abs(),
        ));
    }
    let sch_path = out_dir.join("plan-amortissement-demo.csv");
    std::fs::write(&sch_path, sch).unwrap();

    let runs = assets::list_depreciation_runs(&conn, None).unwrap();
    let mut runs_csv = String::from("Période;Bien;Montant;Journal;Écriture\n");
    for r in runs {
        runs_csv.push_str(&format!(
            "{};{};{},{:02};{};{}\n",
            r.period_label,
            r.asset_code,
            r.amount_cents / 100, (r.amount_cents % 100).abs(),
            r.journal_code.unwrap_or_default(),
            r.ecriture_num.unwrap_or_default(),
        ));
    }
    let runs_path = out_dir.join("dotations-od-demo.csv");
    std::fs::write(&runs_path, runs_csv).unwrap();
    println!("ok\n{}\n{}\n{}", reg_path.display(), sch_path.display(), runs_path.display());
    let _ = open_connection(&workspace);
}
