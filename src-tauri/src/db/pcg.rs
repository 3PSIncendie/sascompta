use super::DbResult;
use rusqlite::{params, Connection};

pub fn seed_journals(conn: &Connection) -> DbResult<()> {
    let journals = [
        ("ACH", "Achats", "purchase"),
        ("VTE", "Ventes", "sales"),
        ("BQ", "Banque", "bank"),
        ("OD", "Opérations diverses", "general"),
        ("AN", "À-nouveaux", "opening"),
    ];
    for (code, label, jtype) in journals {
        conn.execute(
            "INSERT OR IGNORE INTO journal(code, label, journal_type) VALUES (?1, ?2, ?3)",
            params![code, label, jtype],
        )?;
    }
    Ok(())
}

/// PCG SAS usuels — classes 1–7
pub fn seed_accounts(conn: &Connection) -> DbResult<()> {
    let accounts: &[(&str, &str, i32, &str, bool)] = &[
        ("101", "Capital", 1, "equity", false),
        ("106", "Réserves", 1, "equity", false),
        ("120", "Résultat de l'exercice (bénéfice)", 1, "equity", false),
        ("164", "Emprunts auprès des établissements de crédit", 1, "liability", false),
        ("201", "Frais d'établissement", 2, "asset", false),
        ("211", "Terrains", 2, "asset", false),
        ("213", "Constructions", 2, "asset", false),
        ("215", "Installations techniques, matériel et outillage", 2, "asset", false),
        ("218", "Autres immobilisations corporelles", 2, "asset", false),
        ("281", "Amortissements des immobilisations corporelles", 2, "asset", false),
        ("280", "Amortissements des immobilisations incorporelles", 2, "asset", false),
        ("37", "Stocks de marchandises", 3, "asset", false),
        ("401", "Fournisseurs", 4, "liability", true),
        ("408", "Fournisseurs - factures non parvenues", 4, "liability", false),
        ("411", "Clients", 4, "asset", true),
        ("418", "Clients - factures à établir", 4, "asset", false),
        ("4191", "Clients - avances et acomptes reçus", 4, "liability", false),
        ("421", "Personnel - rémunérations dues", 4, "liability", false),
        ("431", "Sécurité sociale", 4, "liability", false),
        ("4452", "TVA due par autoliquidation (BTP / art. 283)", 4, "liability", false),
        ("44566", "TVA déductible sur autres biens et services", 4, "asset", false),
        ("44571", "TVA collectée", 4, "liability", false),
        ("44567", "Crédit de TVA à reporter", 4, "asset", false),
        ("455", "Associés - comptes courants", 4, "liability", false),
        ("467", "Autres comptes débiteurs ou créditeurs", 4, "asset", false),
        ("486", "Charges constatées d'avance", 4, "asset", false),
        ("487", "Produits constatés d'avance", 4, "liability", false),
        ("512", "Banque", 5, "asset", false),
        ("530", "Caisse", 5, "asset", false),
        ("601", "Achats stockés - matières premières", 6, "expense", false),
        ("604", "Achats de sous-traitance", 6, "expense", false),
        ("606", "Achats non stockés de matières et fournitures", 6, "expense", false),
        ("607", "Achats de marchandises", 6, "expense", false),
        ("613", "Locations", 6, "expense", false),
        ("622", "Rémunérations d'intermédiaires et honoraires", 6, "expense", false),
        ("625", "Déplacements, missions et réceptions", 6, "expense", false),
        ("626", "Frais postaux et de télécommunications", 6, "expense", false),
        ("627", "Services bancaires et assimilés", 6, "expense", false),
        ("641", "Rémunérations du personnel", 6, "expense", false),
        ("645", "Charges de sécurité sociale et de prévoyance", 6, "expense", false),
        ("681", "Dotations aux amortissements et provisions", 6, "expense", false),
        ("701", "Ventes de produits finis", 7, "revenue", false),
        ("706", "Prestations de services", 7, "revenue", false),
        ("707", "Ventes de marchandises", 7, "revenue", false),
        ("708", "Produits des activités annexes", 7, "revenue", false),
        ("791", "Transferts de charges", 7, "revenue", false),
    ];

    for (number, label, class, atype, aux) in accounts {
        conn.execute(
            "INSERT OR IGNORE INTO account(number, label, class, account_type, is_auxiliary, is_custom, active)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 1)",
            params![number, label, class, atype, *aux as i32],
        )?;
    }
    Ok(())
}
