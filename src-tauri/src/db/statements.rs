//! États annuels J6 — bilan, compte de résultat, annexes socle.

use super::assets;
use super::reporting::{balance_report, write_text_file};
use super::DbResult;
use crate::models::*;
use printpdf::*;
use rusqlite::Connection;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Actif,
    Passif,
    Charge,
    Produit,
}

struct MappingRule {
    /// Préfixe de numéro de compte (match le plus long gagne).
    prefix: &'static str,
    code: &'static str,
    label: &'static str,
    side: Side,
    /// Sens du solde attendu pour le montant affiché (débit positif pour actif/charge).
    debit_positive: bool,
}

/// Mapping PCG → postes bilan / CR (socle SAS MVP).
const RULES: &[MappingRule] = &[
    // Capitaux
    MappingRule {
        prefix: "101",
        code: "P-CAP",
        label: "Capital social",
        side: Side::Passif,
        debit_positive: false,
    },
    MappingRule {
        prefix: "106",
        code: "P-RES",
        label: "Réserves",
        side: Side::Passif,
        debit_positive: false,
    },
    MappingRule {
        prefix: "12",
        code: "P-RN",
        label: "Report à nouveau",
        side: Side::Passif,
        debit_positive: false,
    },
    // Immos
    MappingRule {
        prefix: "20",
        code: "A-IMMO-INC",
        label: "Immobilisations incorporelles",
        side: Side::Actif,
        debit_positive: true,
    },
    MappingRule {
        prefix: "21",
        code: "A-IMMO-COR",
        label: "Immobilisations corporelles (brut)",
        side: Side::Actif,
        debit_positive: true,
    },
    MappingRule {
        prefix: "28",
        code: "A-AMORT",
        label: "Amortissements (à déduire)",
        side: Side::Actif,
        debit_positive: false, // crédit → montant positif déduit
    },
    MappingRule {
        prefix: "23",
        code: "A-IMMO-EC",
        label: "Immobilisations en cours",
        side: Side::Actif,
        debit_positive: true,
    },
    // Tiers & TVA
    MappingRule {
        prefix: "401",
        code: "P-FOUR",
        label: "Dettes fournisseurs",
        side: Side::Passif,
        debit_positive: false,
    },
    MappingRule {
        prefix: "411",
        code: "A-CLI",
        label: "Créances clients",
        side: Side::Actif,
        debit_positive: true,
    },
    MappingRule {
        prefix: "44566",
        code: "A-TVA-D",
        label: "TVA déductible",
        side: Side::Actif,
        debit_positive: true,
    },
    MappingRule {
        prefix: "44571",
        code: "P-TVA-C",
        label: "TVA collectée",
        side: Side::Passif,
        debit_positive: false,
    },
    MappingRule {
        prefix: "445",
        code: "P-TVA-AUT",
        label: "Autres comptes de TVA",
        side: Side::Passif,
        debit_positive: false,
    },
    MappingRule {
        prefix: "421",
        code: "P-PERS",
        label: "Personnel — rémunérations dues",
        side: Side::Passif,
        debit_positive: false,
    },
    MappingRule {
        prefix: "43",
        code: "P-SOC",
        label: "Sécurité sociale et autres organismes",
        side: Side::Passif,
        debit_positive: false,
    },
    MappingRule {
        prefix: "16",
        code: "P-EMPR",
        label: "Emprunts et dettes assimilées",
        side: Side::Passif,
        debit_positive: false,
    },
    // Trésorerie
    MappingRule {
        prefix: "512",
        code: "A-BQ",
        label: "Banque",
        side: Side::Actif,
        debit_positive: true,
    },
    MappingRule {
        prefix: "53",
        code: "A-CAI",
        label: "Caisse",
        side: Side::Actif,
        debit_positive: true,
    },
    MappingRule {
        prefix: "5",
        code: "A-TRESO",
        label: "Autres disponibilités",
        side: Side::Actif,
        debit_positive: true,
    },
    // Charges
    MappingRule {
        prefix: "60",
        code: "C-ACH",
        label: "Achats",
        side: Side::Charge,
        debit_positive: true,
    },
    MappingRule {
        prefix: "61",
        code: "C-EXT",
        label: "Services extérieurs",
        side: Side::Charge,
        debit_positive: true,
    },
    MappingRule {
        prefix: "62",
        code: "C-EXT2",
        label: "Autres services extérieurs",
        side: Side::Charge,
        debit_positive: true,
    },
    MappingRule {
        prefix: "63",
        code: "C-IMP",
        label: "Impôts et taxes",
        side: Side::Charge,
        debit_positive: true,
    },
    MappingRule {
        prefix: "64",
        code: "C-PERS",
        label: "Charges de personnel",
        side: Side::Charge,
        debit_positive: true,
    },
    MappingRule {
        prefix: "65",
        code: "C-AUT",
        label: "Autres charges de gestion courante",
        side: Side::Charge,
        debit_positive: true,
    },
    MappingRule {
        prefix: "66",
        code: "C-FIN",
        label: "Charges financières",
        side: Side::Charge,
        debit_positive: true,
    },
    MappingRule {
        prefix: "67",
        code: "C-EXC",
        label: "Charges exceptionnelles",
        side: Side::Charge,
        debit_positive: true,
    },
    MappingRule {
        prefix: "68",
        code: "C-DOT",
        label: "Dotations aux amortissements et provisions",
        side: Side::Charge,
        debit_positive: true,
    },
    MappingRule {
        prefix: "69",
        code: "C-IS",
        label: "Participation et impôts sur les bénéfices",
        side: Side::Charge,
        debit_positive: true,
    },
    MappingRule {
        prefix: "6",
        code: "C-DIV",
        label: "Autres charges",
        side: Side::Charge,
        debit_positive: true,
    },
    // Produits
    MappingRule {
        prefix: "70",
        code: "R-VTE",
        label: "Ventes de produits fabriqués, prestations, marchandises",
        side: Side::Produit,
        debit_positive: false,
    },
    MappingRule {
        prefix: "71",
        code: "R-PROD",
        label: "Production stockée / immobilisée",
        side: Side::Produit,
        debit_positive: false,
    },
    MappingRule {
        prefix: "74",
        code: "R-SUB",
        label: "Subventions d'exploitation",
        side: Side::Produit,
        debit_positive: false,
    },
    MappingRule {
        prefix: "75",
        code: "R-AUT",
        label: "Autres produits de gestion courante",
        side: Side::Produit,
        debit_positive: false,
    },
    MappingRule {
        prefix: "76",
        code: "R-FIN",
        label: "Produits financiers",
        side: Side::Produit,
        debit_positive: false,
    },
    MappingRule {
        prefix: "77",
        code: "R-EXC",
        label: "Produits exceptionnels",
        side: Side::Produit,
        debit_positive: false,
    },
    MappingRule {
        prefix: "78",
        code: "R-REP",
        label: "Reprises sur amortissements et provisions",
        side: Side::Produit,
        debit_positive: false,
    },
    MappingRule {
        prefix: "7",
        code: "R-DIV",
        label: "Autres produits",
        side: Side::Produit,
        debit_positive: false,
    },
];

fn find_rule(account_number: &str) -> Option<&'static MappingRule> {
    let mut best: Option<&MappingRule> = None;
    for rule in RULES {
        if account_number.starts_with(rule.prefix) {
            if best.map(|b| rule.prefix.len() > b.prefix.len()).unwrap_or(true) {
                best = Some(rule);
            }
        }
    }
    best
}

fn cents_fr(cents: i64) -> String {
    crate::db::tax::format_cents_csv(cents)
}

fn ascii_safe(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'À' | 'Â' | 'Ä' => 'A',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'Î' | 'Ï' => 'I',
            'Ô' | 'Ö' => 'O',
            'Ù' | 'Û' | 'Ü' => 'U',
            'Ç' => 'C',
            '€' => 'E',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

fn signed_amount(row: &BalanceRow, debit_positive: bool) -> i64 {
    let net = row.solde_debit_cents - row.solde_credit_cents;
    if debit_positive {
        net
    } else {
        -net
    }
}

fn aggregate_posts(
    balance: &BalanceReport,
    side_filter: Side,
) -> Vec<(String, String, i64, Vec<String>)> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, (String, i64, Vec<String>)> = BTreeMap::new();
    for row in &balance.rows {
        let Some(rule) = find_rule(&row.account_number) else {
            continue;
        };
        if rule.side != side_filter {
            continue;
        }
        let amt = signed_amount(row, rule.debit_positive);
        if amt == 0 {
            continue;
        }
        let e = map
            .entry(rule.code.to_string())
            .or_insert((rule.label.to_string(), 0, Vec::new()));
        e.1 += amt;
        e.2.push(row.account_number.clone());
    }
    map.into_iter()
        .map(|(code, (label, amount, accounts))| (code, label, amount, accounts))
        .collect()
}

fn line_from(
    code: &str,
    label: &str,
    amount: i64,
    accounts: &[String],
    note: Option<&str>,
) -> StatementLine {
    StatementLine {
        code: code.to_string(),
        label: label.to_string(),
        amount_cents: amount,
        accounts: accounts.to_vec(),
        note: note.map(|s| s.to_string()),
    }
}

pub fn year_filter(from: &str, to: &str) -> LedgerFilter {
    LedgerFilter {
        from_date: Some(from.to_string()),
        to_date: Some(to.to_string()),
        journal_code: None,
        account_number: None,
    }
}

pub fn build_income_statement(conn: &Connection, filter: &LedgerFilter) -> DbResult<IncomeStatement> {
    let balance = balance_report(conn, filter)?;
    let charges_raw = aggregate_posts(&balance, Side::Charge);
    let produits_raw = aggregate_posts(&balance, Side::Produit);

    let charges: Vec<StatementLine> = charges_raw
        .iter()
        .map(|(c, l, a, acc)| line_from(c, l, *a, acc, None))
        .collect();
    let produits: Vec<StatementLine> = produits_raw
        .iter()
        .map(|(c, l, a, acc)| line_from(c, l, *a, acc, None))
        .collect();

    let total_charges: i64 = charges.iter().map(|l| l.amount_cents).sum();
    let total_produits: i64 = produits.iter().map(|l| l.amount_cents).sum();
    let resultat = total_produits - total_charges;

    Ok(IncomeStatement {
        from_date: filter.from_date.clone().unwrap_or_default(),
        to_date: filter.to_date.clone().unwrap_or_default(),
        charges,
        produits,
        total_charges_cents: total_charges,
        total_produits_cents: total_produits,
        resultat_cents: resultat,
    })
}

pub fn build_balance_sheet(conn: &Connection, filter: &LedgerFilter) -> DbResult<BalanceSheet> {
    let balance = balance_report(conn, filter)?;
    let income = build_income_statement(conn, filter)?;

    let actif_raw = aggregate_posts(&balance, Side::Actif);
    let mut actif_lines = Vec::new();
    let mut amort = 0i64;
    for (code, label, amount, accounts) in &actif_raw {
        if code == "A-AMORT" {
            amort += *amount;
        } else {
            actif_lines.push(line_from(code, label, *amount, accounts, None));
        }
    }
    if amort != 0 {
        actif_lines.push(line_from(
            "A-AMORT",
            "Amortissements immobilisations corporelles",
            -amort,
            &[],
            Some("À déduire du brut"),
        ));
    }
    actif_lines.sort_by(|a, b| a.code.cmp(&b.code));

    let mut passif_lines: Vec<StatementLine> = aggregate_posts(&balance, Side::Passif)
        .into_iter()
        .map(|(c, l, a, acc)| line_from(&c, &l, a, &acc, None))
        .collect();

    if income.resultat_cents >= 0 {
        passif_lines.push(line_from(
            "P-RESUL",
            "Résultat de l'exercice (bénéfice)",
            income.resultat_cents,
            &[],
            Some("Issu du compte de résultat"),
        ));
    } else {
        actif_lines.push(line_from(
            "A-PERTE",
            "Résultat de l'exercice (perte)",
            -income.resultat_cents,
            &[],
            Some("Issu du compte de résultat"),
        ));
    }
    passif_lines.sort_by(|a, b| a.code.cmp(&b.code));

    let total_actif: i64 = actif_lines.iter().map(|l| l.amount_cents).sum();
    let total_passif: i64 = passif_lines.iter().map(|l| l.amount_cents).sum();

    Ok(BalanceSheet {
        as_of: filter.to_date.clone().unwrap_or_default(),
        from_date: filter.from_date.clone().unwrap_or_default(),
        to_date: filter.to_date.clone().unwrap_or_default(),
        actif: actif_lines,
        passif: passif_lines,
        total_actif_cents: total_actif,
        total_passif_cents: total_passif,
        ecart_cents: total_actif - total_passif,
        resultat_cents: income.resultat_cents,
    })
}

pub fn build_annex(conn: &Connection, filter: &LedgerFilter) -> DbResult<AnnualAnnex> {
    let balance = balance_report(conn, filter)?;
    let fixed = assets::list_assets(conn)?;
    let runs = assets::list_depreciation_runs(conn, None)?;

    let methods = vec![
        "Comptes annuels établis selon le PCG, méthode de la partie double.".into(),
        "Amortissements linéaires sur la durée d'utilité (pas de prorata temporis fin de mois au MVP).".into(),
        "Exercice civil ; montants en euros.".into(),
        "Annexe socle MVP — hors engagements hors bilan détaillés.".into(),
    ];

    let mut immos = Vec::new();
    for a in &fixed {
        let cumul: i64 = runs
            .iter()
            .filter(|r| r.asset_id == a.id)
            .map(|r| r.amount_cents)
            .sum();
        immos.push(AnnexAssetRow {
            code: a.code.clone(),
            label: a.label.clone(),
            account_number: a.account_number.clone(),
            gross_cents: a.base_cents,
            cumulative_amort_cents: cumul,
            net_cents: a.base_cents - cumul,
            useful_life_years: a.useful_life_years,
            method: a.method.clone(),
        });
    }

    let mut debts = Vec::new();
    for row in &balance.rows {
        let Some(rule) = find_rule(&row.account_number) else {
            continue;
        };
        if rule.side != Side::Passif {
            continue;
        }
        // Dettes = passif hors capital / réserves / résultat
        if rule.code.starts_with("P-CAP")
            || rule.code.starts_with("P-RES")
            || rule.code.starts_with("P-RN")
            || rule.code == "P-RESUL"
        {
            continue;
        }
        let amt = signed_amount(row, rule.debit_positive);
        if amt <= 0 {
            continue;
        }
        debts.push(AnnexDebtRow {
            account_number: row.account_number.clone(),
            label: row.account_label.clone(),
            amount_cents: amt,
            nature: rule.label.to_string(),
        });
    }
    debts.sort_by(|a, b| a.account_number.cmp(&b.account_number));

    Ok(AnnualAnnex {
        as_of: filter.to_date.clone().unwrap_or_default(),
        methods,
        immobilisations: immos,
        dettes: debts,
    })
}

pub fn build_annual_pack(conn: &Connection, filter: &LedgerFilter) -> DbResult<AnnualStatements> {
    let org: Organization = conn.query_row(
        "SELECT id, name, siren, legal_form, vat_regime, address, capital_cents FROM organization WHERE id = 1",
        [],
        |r| {
            Ok(Organization {
                id: r.get(0)?,
                name: r.get(1)?,
                siren: r.get(2)?,
                legal_form: r.get(3)?,
                vat_regime: r.get(4)?,
                address: r.get(5)?,
                capital_cents: r.get(6)?,
            })
        },
    )?;
    Ok(AnnualStatements {
        organization_name: org.name,
        siren: org.siren,
        bilan: build_balance_sheet(conn, filter)?,
        compte_resultat: build_income_statement(conn, filter)?,
        annexe: build_annex(conn, filter)?,
    })
}

pub fn export_bilan_csv(sheet: &BalanceSheet) -> String {
    let mut out = String::from('\u{FEFF}');
    out.push_str("Côté;Code;Libellé;Montant;Comptes;Note\n");
    for l in &sheet.actif {
        out.push_str(&format!(
            "Actif;{};{};{};{};{}\n",
            l.code,
            l.label.replace(';', ","),
            cents_fr(l.amount_cents),
            l.accounts.join("+"),
            l.note.as_deref().unwrap_or("").replace(';', ","),
        ));
    }
    out.push_str(&format!(
        "Actif;TOTAL;Total actif;{};\n",
        cents_fr(sheet.total_actif_cents)
    ));
    for l in &sheet.passif {
        out.push_str(&format!(
            "Passif;{};{};{};{};{}\n",
            l.code,
            l.label.replace(';', ","),
            cents_fr(l.amount_cents),
            l.accounts.join("+"),
            l.note.as_deref().unwrap_or("").replace(';', ","),
        ));
    }
    out.push_str(&format!(
        "Passif;TOTAL;Total passif;{};\n",
        cents_fr(sheet.total_passif_cents)
    ));
    out.push_str(&format!(
        "Contrôle;ECART;Actif − Passif;{};\n",
        cents_fr(sheet.ecart_cents)
    ));
    out.push_str(&format!(
        "CR;RESULTAT;Résultat de l'exercice;{};\n",
        cents_fr(sheet.resultat_cents)
    ));
    out
}

pub fn export_cr_csv(cr: &IncomeStatement) -> String {
    let mut out = String::from('\u{FEFF}');
    out.push_str("Nature;Code;Libellé;Montant;Comptes\n");
    for l in &cr.produits {
        out.push_str(&format!(
            "Produit;{};{};{};{}\n",
            l.code,
            l.label.replace(';', ","),
            cents_fr(l.amount_cents),
            l.accounts.join("+"),
        ));
    }
    out.push_str(&format!(
        "Produit;TOTAL;Total produits;{};\n",
        cents_fr(cr.total_produits_cents)
    ));
    for l in &cr.charges {
        out.push_str(&format!(
            "Charge;{};{};{};{}\n",
            l.code,
            l.label.replace(';', ","),
            cents_fr(l.amount_cents),
            l.accounts.join("+"),
        ));
    }
    out.push_str(&format!(
        "Charge;TOTAL;Total charges;{};\n",
        cents_fr(cr.total_charges_cents)
    ));
    out.push_str(&format!(
        "Résultat;RESULTAT;Résultat net;{};\n",
        cents_fr(cr.resultat_cents)
    ));
    out
}

pub fn export_annexe_csv(annexe: &AnnualAnnex) -> String {
    let mut out = String::from('\u{FEFF}');
    out.push_str("Section;Code;Libellé;Brut;Amort;Net;Extra\n");
    for m in &annexe.methods {
        out.push_str(&format!("Méthode;;{};;;;\n", m.replace(';', ",")));
    }
    for a in &annexe.immobilisations {
        out.push_str(&format!(
            "Immo;{};{};{};{};{};{} ans {}\n",
            a.code,
            a.label.replace(';', ","),
            cents_fr(a.gross_cents),
            cents_fr(a.cumulative_amort_cents),
            cents_fr(a.net_cents),
            a.useful_life_years,
            a.method,
        ));
    }
    for d in &annexe.dettes {
        out.push_str(&format!(
            "Dette;{};{};{};;;{};\n",
            d.account_number,
            d.label.replace(';', ","),
            cents_fr(d.amount_cents),
            d.nature.replace(';', ","),
        ));
    }
    out
}

fn pdf_write_lines(
    doc: &PdfDocumentReference,
    font: &IndirectFontRef,
    layer: &PdfLayerReference,
    start_y: f32,
    lines: &[String],
) -> DbResult<(PdfLayerReference, f32)> {
    let mut current = layer.clone();
    let mut y = start_y;
    for line in lines {
        if y < 20.0 {
            let (p, l) = doc.add_page(Mm(210.0), Mm(297.0), "Layer");
            current = doc.get_page(p).get_layer(l);
            y = 280.0;
        }
        current.use_text(ascii_safe(line), 9.0, Mm(15.0), Mm(y), font);
        y -= 5.5;
    }
    Ok((current, y))
}

pub fn export_annual_pdf(path: &Path, pack: &AnnualStatements) -> DbResult<()> {
    let (doc, page1, layer1) =
        PdfDocument::new("SasCompta Comptes annuels", Mm(210.0), Mm(297.0), "L1");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("PDF font: {e}"))?;
    let mut layer = doc.get_page(page1).get_layer(layer1);
    let mut y = 280.0;

    let mut lines = Vec::new();
    lines.push(format!(
        "Comptes annuels — {} (SIREN {})",
        pack.organization_name, pack.siren
    ));
    lines.push(format!(
        "Exercice du {} au {}",
        pack.bilan.from_date, pack.bilan.to_date
    ));
    lines.push(String::new());
    lines.push("--- BILAN ACTIF ---".into());
    for l in &pack.bilan.actif {
        lines.push(format!(
            "  {} {}  {}",
            l.code,
            l.label,
            cents_fr(l.amount_cents)
        ));
    }
    lines.push(format!(
        "  TOTAL ACTIF  {}",
        cents_fr(pack.bilan.total_actif_cents)
    ));
    lines.push(String::new());
    lines.push("--- BILAN PASSIF ---".into());
    for l in &pack.bilan.passif {
        lines.push(format!(
            "  {} {}  {}",
            l.code,
            l.label,
            cents_fr(l.amount_cents)
        ));
    }
    lines.push(format!(
        "  TOTAL PASSIF  {}",
        cents_fr(pack.bilan.total_passif_cents)
    ));
    lines.push(format!("  Ecart actif-passif  {}", cents_fr(pack.bilan.ecart_cents)));
    lines.push(String::new());
    lines.push("--- COMPTE DE RESULTAT ---".into());
    lines.push("Produits:".into());
    for l in &pack.compte_resultat.produits {
        lines.push(format!(
            "  {} {}  {}",
            l.code,
            l.label,
            cents_fr(l.amount_cents)
        ));
    }
    lines.push(format!(
        "  Total produits  {}",
        cents_fr(pack.compte_resultat.total_produits_cents)
    ));
    lines.push("Charges:".into());
    for l in &pack.compte_resultat.charges {
        lines.push(format!(
            "  {} {}  {}",
            l.code,
            l.label,
            cents_fr(l.amount_cents)
        ));
    }
    lines.push(format!(
        "  Total charges  {}",
        cents_fr(pack.compte_resultat.total_charges_cents)
    ));
    lines.push(format!(
        "  RESULTAT  {}",
        cents_fr(pack.compte_resultat.resultat_cents)
    ));
    lines.push(String::new());
    lines.push("--- ANNEXE (socle) ---".into());
    for m in &pack.annexe.methods {
        lines.push(format!("  * {m}"));
    }
    lines.push("Immobilisations:".into());
    for a in &pack.annexe.immobilisations {
        lines.push(format!(
            "  {} {} brut {} amort {} net {}",
            a.code,
            a.label,
            cents_fr(a.gross_cents),
            cents_fr(a.cumulative_amort_cents),
            cents_fr(a.net_cents)
        ));
    }
    lines.push("Dettes:".into());
    for d in &pack.annexe.dettes {
        lines.push(format!(
            "  {} {}  {} ({})",
            d.account_number,
            d.label,
            cents_fr(d.amount_cents),
            d.nature
        ));
    }

    let (new_layer, new_y) = pdf_write_lines(&doc, &font, &layer, y, &lines)?;
    layer = new_layer;
    y = new_y;
    let _ = (layer, y);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    doc.save(&mut BufWriter::new(File::create(path)?))
        .map_err(|e| format!("PDF save: {e}"))?;
    Ok(())
}

pub fn export_pack_files(dir: &Path, pack: &AnnualStatements) -> DbResult<()> {
    std::fs::create_dir_all(dir)?;
    write_text_file(&dir.join("bilan.csv"), &export_bilan_csv(&pack.bilan))?;
    write_text_file(
        &dir.join("compte-resultat.csv"),
        &export_cr_csv(&pack.compte_resultat),
    )?;
    write_text_file(&dir.join("annexe.csv"), &export_annexe_csv(&pack.annexe))?;
    export_annual_pdf(&dir.join("comptes-annuels.pdf"), pack)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_demo_workspace, open_connection};
    use tempfile::tempdir;

    fn year_filter() -> LedgerFilter {
        LedgerFilter {
            from_date: Some("2026-01-01".into()),
            to_date: Some("2026-12-31".into()),
            journal_code: None,
            account_number: None,
        }
    }

    #[test]
    fn demo_bilan_cr_coherent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("j6.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        let f = year_filter();

        let cr = build_income_statement(&conn, &f).unwrap();
        // Produits 13 000 ; charges 7 325 → résultat 5 675
        assert_eq!(cr.total_produits_cents, 1_300_000);
        assert_eq!(cr.total_charges_cents, 732_500);
        assert_eq!(cr.resultat_cents, 567_500);

        let bilan = build_balance_sheet(&conn, &f).unwrap();
        assert_eq!(bilan.ecart_cents, 0, "actif doit égaler passif");
        assert_eq!(bilan.resultat_cents, 567_500);
        // Totaux attendus du jeu de test (avec acquisition IMMO 5 000 €)
        assert_eq!(bilan.total_actif_cents, 6_523_500);
        assert_eq!(bilan.total_passif_cents, 6_523_500);

        let annexe = build_annex(&conn, &f).unwrap();
        assert!(!annexe.immobilisations.is_empty());
        assert_eq!(annexe.immobilisations[0].gross_cents, 500_000);
        assert_eq!(annexe.immobilisations[0].cumulative_amort_cents, 50_000);
        assert_eq!(annexe.immobilisations[0].net_cents, 450_000);
        assert!(!annexe.dettes.is_empty());

        let pack = build_annual_pack(&conn, &f).unwrap();
        let out = dir.path().join("pack");
        export_pack_files(&out, &pack).unwrap();
        assert!(out.join("bilan.csv").exists());
        assert!(out.join("compte-resultat.csv").exists());
        assert!(out.join("annexe.csv").exists());
        assert!(out.join("comptes-annuels.pdf").exists());
    }

    #[test]
    fn mapping_longest_prefix() {
        assert_eq!(find_rule("44566").unwrap().code, "A-TVA-D");
        assert_eq!(find_rule("44571").unwrap().code, "P-TVA-C");
        assert_eq!(find_rule("215").unwrap().code, "A-IMMO-COR");
        assert_eq!(find_rule("681").unwrap().code, "C-DOT");
        assert_eq!(find_rule("707").unwrap().code, "R-VTE");
    }
}
