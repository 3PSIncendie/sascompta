use sascompta_lib::db;
use std::env;
use std::path::PathBuf;

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut p = env::temp_dir();
            p.push("sascompta-demo.sascompta");
            p
        });
    match db::create_demo_workspace(&path) {
        Ok(info) => {
            println!("Workspace démo créé : {}", path.display());
            println!("Société : {} ({})", info.organization.name, info.organization.siren);
            println!("Exercice : {}", info.fiscal_year.label);
            println!("Mot de passe : {}", db::DEMO_PASSWORD);
        }
        Err(e) => {
            eprintln!("Erreur : {e}");
            std::process::exit(1);
        }
    }
}
