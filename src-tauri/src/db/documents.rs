//! Import de documents → brouillon d’écriture (revue humaine obligatoire).
//!
//! Types pris en charge : facture fournisseur (ACH), facture client (VTE),
//! note de frais / reçu (ACH ou OD), ticket de caisse (OD/ACH), relevé bancaire (BQ).
//! Flux commun : analyse locale → revue → confirmation → `create_draft` + pièce jointe.

use super::ledger;
use super::tax;
use super::DbResult;
use crate::models::*;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub const DOC_TYPES: &[&str] = &[
    "supplier_invoice",
    "customer_invoice",
    "expense_receipt",
    "bank_statement",
    "ticket",
    "unknown",
];

/// Types pour lesquels un modèle d’écriture est implémenté.
pub fn posting_supported(doc_type: &str) -> bool {
    matches!(
        doc_type,
        "supplier_invoice"
            | "customer_invoice"
            | "expense_receipt"
            | "bank_statement"
            | "ticket"
    )
}

pub fn default_journal_for(doc_type: &str) -> &'static str {
    match doc_type {
        "supplier_invoice" => "ACH",
        "customer_invoice" => "VTE",
        "expense_receipt" => "ACH",
        "bank_statement" => "BQ",
        "ticket" => "OD",
        _ => "OD",
    }
}

pub fn default_ht_account_for(doc_type: &str, reverse: bool) -> &'static str {
    match doc_type {
        "supplier_invoice" if reverse => "604",
        "supplier_invoice" => "606",
        "customer_invoice" => "706",
        "expense_receipt" | "ticket" => "625",
        "bank_statement" => "471",
        _ => "606",
    }
}

pub fn default_counterpart_for(doc_type: &str) -> &'static str {
    match doc_type {
        "supplier_invoice" | "expense_receipt" => "401",
        "customer_invoice" => "411",
        "ticket" => "530",
        "bank_statement" => "512",
        _ => "401",
    }
}

pub fn migrate_documents(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS document_import (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            doc_type TEXT NOT NULL
                CHECK (doc_type IN (
                    'supplier_invoice','customer_invoice','expense_receipt',
                    'bank_statement','ticket','unknown'
                )),
            source_file_name TEXT NOT NULL,
            stored_path TEXT NOT NULL,
            mime_type TEXT,
            extraction_method TEXT NOT NULL DEFAULT 'none'
                CHECK (extraction_method IN (
                    'pdf_text','image_pending_ocr','manual','none'
                )),
            extraction_json TEXT NOT NULL,
            confidence REAL NOT NULL DEFAULT 0,
            status TEXT NOT NULL DEFAULT 'pending_review'
                CHECK (status IN ('pending_review','confirmed','discarded')),
            entry_id INTEGER REFERENCES entry(id),
            created_at TEXT NOT NULL,
            confirmed_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_document_import_status
            ON document_import(status);
        "#,
    )?;
    Ok(())
}

/// Dossier pièces à côté du fichier workspace : `societe.sascompta.files/imports/`.
pub fn imports_dir(workspace_path: &Path) -> PathBuf {
    let mut base = workspace_path.to_path_buf();
    let name = base
        .file_name()
        .map(|s| format!("{}.files", s.to_string_lossy()))
        .unwrap_or_else(|| "workspace.files".into());
    base.set_file_name(name);
    base.join("imports")
}

fn guess_mime(path: &Path) -> String {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => "application/pdf".into(),
        "png" => "image/png".into(),
        "jpg" | "jpeg" => "image/jpeg".into(),
        "webp" => "image/webp".into(),
        "tif" | "tiff" => "image/tiff".into(),
        other => format!("application/octet-stream:{other}"),
    }
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

fn is_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "tif" | "tiff"
    )
}

/// Extraction locale : couche texte PDF d’abord. OCR cloud non requis.
pub fn extract_text(path: &Path) -> (String, String) {
    if is_pdf(path) {
        match pdf_extract::extract_text(path) {
            Ok(text) if text.chars().filter(|c| !c.is_whitespace()).count() >= 40 => {
                return (text, "pdf_text".into());
            }
            Ok(text) if !text.trim().is_empty() => {
                // Texte trop pauvre → probablement scanné ; pas d’OCR cloud par défaut.
                return (text, "image_pending_ocr".into());
            }
            Ok(_) | Err(_) => {
                return (String::new(), "image_pending_ocr".into());
            }
        }
    }
    if is_image(path) {
        // Lot 1 : pas de Tesseract embarqué ; l’utilisateur complète à la revue.
        // Point d’extension OCR local (Tesseract) sans API payante.
        return (String::new(), "image_pending_ocr".into());
    }
    (String::new(), "none".into())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ParsedAmounts {
    pub ht_cents: Option<i64>,
    pub vat_cents: Option<i64>,
    pub ttc_cents: Option<i64>,
    pub rate_bp: Option<i32>,
}

fn parse_fr_amount_to_cents(raw: &str) -> Option<i64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.' || *c == ' ')
        .collect();
    let cleaned = cleaned.replace(' ', "");
    if cleaned.is_empty() {
        return None;
    }
    // 1.234,56 or 1234,56 or 1234.56
    let normalized = if cleaned.contains(',') && cleaned.contains('.') {
        cleaned.replace('.', "").replace(',', ".")
    } else if cleaned.contains(',') {
        cleaned.replace(',', ".")
    } else {
        cleaned
    };
    let value: f64 = normalized.parse().ok()?;
    Some((value * 100.0).round() as i64)
}

fn find_amount_after(text: &str, labels: &[&str]) -> Option<i64> {
    let lower = text.to_lowercase();
    for label in labels {
        let needle = label.to_lowercase();
        if let Some(idx) = lower.find(&needle) {
            let after = &text[idx + label.len()..];
            let window: String = after.chars().take(48).collect();
            // Prefer amounts with currency / decimal
            let re_candidates = window
                .split(|c: char| c == '\n' || c == '\r' || c == '|')
                .flat_map(|part| part.split_whitespace())
                .filter_map(|tok| {
                    let t = tok.trim_matches(|c: char| {
                        !c.is_ascii_digit() && c != ',' && c != '.' && c != ' '
                    });
                    if t.chars().any(|c| c.is_ascii_digit()) {
                        parse_fr_amount_to_cents(t)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            if let Some(v) = re_candidates.into_iter().find(|v| *v > 0) {
                return Some(v);
            }
        }
    }
    None
}

pub fn parse_amounts(text: &str) -> ParsedAmounts {
    let mut ht = find_amount_after(
        text,
        &[
            "total ht",
            "montant ht",
            "net ht",
            "hors taxe",
            "hors taxes",
            "total h.t.",
            "total h.t",
            "ht :",
            "ht:",
        ],
    );
    let mut vat = find_amount_after(
        text,
        &[
            "total tva",
            "montant tva",
            "tva 20",
            "tva 10",
            "tva 5,5",
            "tva :",
            "tva:",
        ],
    );
    let mut ttc = find_amount_after(
        text,
        &[
            "total ttc",
            "net à payer",
            "net a payer",
            "montant ttc",
            "ttc :",
            "ttc:",
        ],
    );

    // Infer missing sides
    if ht.is_none() && ttc.is_some() && vat.is_some() {
        ht = Some(ttc.unwrap() - vat.unwrap());
    }
    if ttc.is_none() && ht.is_some() && vat.is_some() {
        ttc = Some(ht.unwrap() + vat.unwrap());
    }
    if vat.is_none() && ht.is_some() && ttc.is_some() && ttc.unwrap() >= ht.unwrap() {
        vat = Some(ttc.unwrap() - ht.unwrap());
    }

    let rate_bp = match (ht, vat) {
        (Some(h), Some(v)) if h > 0 => {
            let bp = ((v as f64) * 10_000.0 / (h as f64)).round() as i32;
            if (1800..=2200).contains(&bp) {
                Some(2000)
            } else if (900..=1100).contains(&bp) {
                Some(1000)
            } else if (500..=600).contains(&bp) {
                Some(550)
            } else {
                Some(bp)
            }
        }
        _ => {
            let lower = text.to_lowercase();
            if lower.contains("20 %") || lower.contains("20%") || lower.contains("tva 20") {
                Some(2000)
            } else if lower.contains("10 %") || lower.contains("10%") {
                Some(1000)
            } else if lower.contains("5,5") || lower.contains("5.5") {
                Some(550)
            } else {
                None
            }
        }
    };

    ParsedAmounts {
        ht_cents: ht.filter(|v| *v > 0),
        vat_cents: vat.filter(|v| *v >= 0),
        ttc_cents: ttc.filter(|v| *v > 0),
        rate_bp,
    }
}

pub fn parse_date(text: &str) -> Option<String> {
    // dd/mm/yyyy or dd-mm-yyyy — iterate chars (UTF-8 safe)
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i + 10 <= n {
        let window = &chars[i..i + 10];
        if window[0].is_ascii_digit()
            && window[1].is_ascii_digit()
            && (window[2] == '/' || window[2] == '-')
            && window[3].is_ascii_digit()
            && window[4].is_ascii_digit()
            && (window[5] == '/' || window[5] == '-')
            && window[6].is_ascii_digit()
            && window[7].is_ascii_digit()
            && window[8].is_ascii_digit()
            && window[9].is_ascii_digit()
        {
            let d: String = window[0..2].iter().collect();
            let m: String = window[3..5].iter().collect();
            let y: String = window[6..10].iter().collect();
            if let (Ok(day), Ok(month), Ok(year)) =
                (d.parse::<u32>(), m.parse::<u32>(), y.parse::<u32>())
            {
                if (1..=31).contains(&day)
                    && (1..=12).contains(&month)
                    && (1990..=2100).contains(&year)
                {
                    return Some(format!("{year:04}-{month:02}-{day:02}"));
                }
            }
        }
        i += 1;
    }
    // ISO yyyy-mm-dd
    for line in text.split_whitespace() {
        let cand: String = line.chars().take(10).collect();
        if cand.len() == 10 {
            if let Ok(d) = chrono::NaiveDate::parse_from_str(&cand, "%Y-%m-%d") {
                return Some(d.format("%Y-%m-%d").to_string());
            }
        }
    }
    None
}

pub fn parse_invoice_number(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for label in [
        "n° de facture",
        "no de facture",
        "n° facture",
        "facture n°",
        "facture no",
        "facture:",
        "invoice no",
        "invoice #",
    ] {
        if let Some(idx) = lower.find(label) {
            // idx is byte offset into `lower` (= same as `text` for these ASCII/° labels)
            let after = text
                .get(idx + label.len()..)
                .unwrap_or("")
                .trim_start_matches(|c: char| {
                    c.is_whitespace() || c == ':' || c == '-' || c == '°'
                });
            let num: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '/')
                .collect();
            let num = num.trim().to_string();
            if num.len() >= 2 {
                return Some(num);
            }
        }
    }
    None
}

pub fn parse_siret(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit() || c.is_whitespace()).collect();
    // Look for 14 consecutive digits near "siret"
    let lower = text.to_lowercase();
    if let Some(idx) = lower.find("siret") {
        let window: String = text[idx..].chars().take(40).collect();
        let only: String = window.chars().filter(|c| c.is_ascii_digit()).collect();
        if only.len() >= 14 {
            return Some(only[..14].to_string());
        }
        if only.len() >= 9 {
            return Some(only[..9].to_string());
        }
    }
    // Fallback: any 14-digit run
    let flat: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    if flat.len() >= 14 {
        // Prefer not to guess randomly — skip unless labeled. Use digits near supplier block only.
        let _ = digits;
    }
    None
}

pub fn parse_supplier_name(text: &str) -> Option<String> {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| l.len() >= 3 && l.len() <= 80)
        .collect();
    for line in &lines {
        let lower = line.to_lowercase();
        if lower.contains("facture")
            || lower.contains("total")
            || lower.contains("tva")
            || lower.contains("page")
            || lower.contains("siret")
            || lower.contains("siren")
            || lower.starts_with("date")
        {
            continue;
        }
        // Prefer lines with SAS / SARL / EURL etc.
        if lower.contains("sas")
            || lower.contains("sarl")
            || lower.contains("eurl")
            || lower.contains("sa ")
            || lower.contains("sci")
        {
            return Some((*line).to_string());
        }
    }
    lines.first().map(|s| (*s).to_string())
}

pub fn detect_reverse_charge(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("autoliquidation")
        || lower.contains("auto-liquidation")
        || lower.contains("art. 283")
        || lower.contains("article 283")
        || lower.contains("tva due par le preneur")
        || lower.contains("autoliquide")
}

/// Classifiers typés — score indépendant ; le meilleur gagne si confiance ≥ seuil.
pub fn classify_document(text: &str) -> (String, f64) {
    let lower = text.to_lowercase();
    let mut scores: Vec<(&str, f64)> = DOC_TYPES
        .iter()
        .copied()
        .map(|t| (t, 0.0))
        .collect();

    for (t, s) in scores.iter_mut() {
        *s = match *t {
            "supplier_invoice" => {
                let mut v: f64 = 0.0;
                if lower.contains("facture") {
                    v += 0.3;
                }
                if lower.contains("fournisseur")
                    || lower.contains("à payer")
                    || lower.contains("a payer")
                    || lower.contains("net à payer")
                    || lower.contains("net a payer")
                {
                    v += 0.2;
                }
                if lower.contains("tva") {
                    v += 0.1;
                }
                if lower.contains("total ht") || lower.contains("total ttc") {
                    v += 0.2;
                }
                if lower.contains("siret") {
                    v += 0.1;
                }
                if detect_reverse_charge(&lower) {
                    v += 0.15;
                }
                // Signaux client → baisser le score fournisseur
                if lower.contains("à votre attention")
                    || lower.contains("conditions de règlement")
                    || lower.contains("merci de votre confiance")
                {
                    v -= 0.25;
                }
                v.max(0.0).min(0.98)
            }
            "customer_invoice" => {
                let mut v: f64 = 0.0;
                if lower.contains("facture") {
                    v += 0.25;
                }
                if lower.contains("client")
                    || lower.contains("à votre attention")
                    || lower.contains("a votre attention")
                {
                    v += 0.3;
                }
                if lower.contains("conditions de règlement")
                    || lower.contains("conditions de reglement")
                    || lower.contains("merci de votre confiance")
                {
                    v += 0.15;
                }
                if lower.contains("total ht") || lower.contains("total ttc") {
                    v += 0.15;
                }
                if lower.contains("à payer") || lower.contains("a payer") || lower.contains("fournisseur")
                {
                    v -= 0.2;
                }
                v.max(0.0).min(0.92)
            }
            "expense_receipt" => {
                let mut v: f64 = 0.0;
                if lower.contains("note de frais") || lower.contains("notes de frais") {
                    v += 0.55;
                }
                if lower.contains("reçu") || lower.contains("recu ") || lower.starts_with("recu") {
                    v += 0.35;
                }
                if lower.contains("justificatif") || lower.contains("remboursement frais") {
                    v += 0.25;
                }
                if lower.contains("restaurant")
                    || lower.contains("hôtel")
                    || lower.contains("hotel")
                    || lower.contains("taxi")
                    || lower.contains("péage")
                    || lower.contains("peage")
                {
                    v += 0.15;
                }
                if lower.contains("merci de votre visite") {
                    v += 0.15;
                }
                v.min(0.9)
            }
            "bank_statement" => {
                let mut v: f64 = 0.0;
                if lower.contains("relevé de compte")
                    || lower.contains("releve de compte")
                    || lower.contains("relevé bancaire")
                    || lower.contains("releve bancaire")
                {
                    v += 0.55;
                }
                if lower.contains("relevé") && lower.contains("compte") {
                    v += 0.35;
                }
                if lower.contains("iban") {
                    v += 0.15;
                }
                if lower.contains("solde")
                    && (lower.contains("précédent")
                        || lower.contains("precedent")
                        || lower.contains("nouveau solde")
                        || lower.contains("solde final"))
                {
                    v += 0.25;
                }
                if lower.contains("bic") && lower.contains("iban") {
                    v += 0.1;
                }
                v.min(0.95)
            }
            "ticket" => {
                let mut v: f64 = 0.0;
                if lower.contains("ticket de caisse") {
                    v += 0.5;
                }
                if lower.contains("ticket") {
                    v += 0.25;
                }
                if lower.contains("cb ")
                    || lower.contains("carte bancaire")
                    || lower.contains("carte bleue")
                    || lower.contains("sans contact")
                    || lower.contains("tpe")
                {
                    v += 0.2;
                }
                if lower.contains("merci de votre visite") || lower.contains("à bientôt") {
                    v += 0.1;
                }
                // Note de frais explicite gagne sur ticket
                if lower.contains("note de frais") {
                    v -= 0.3;
                }
                v.max(0.0).min(0.88)
            }
            _ => 0.05,
        };
    }

    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let (best, conf) = scores[0];
    if conf < 0.45 {
        ("unknown".into(), conf)
    } else {
        (best.to_string(), conf)
    }
}

fn match_third_party(
    conn: &Connection,
    kind: &str,
    name: Option<&str>,
    siret: Option<&str>,
) -> DbResult<Option<ThirdParty>> {
    if let Some(s) = siret {
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 9 {
            let found = conn
                .query_row(
                    "SELECT id FROM third_party WHERE kind = ?1
                     AND replace(coalesce(siret,''), ' ', '') LIKE ?2 || '%'
                     LIMIT 1",
                    params![kind, digits],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?;
            if let Some(id) = found {
                return Ok(tax::list_third_parties(conn)?
                    .into_iter()
                    .find(|t| t.id == id));
            }
        }
    }
    if let Some(n) = name {
        let needle = n.trim().to_lowercase();
        if needle.len() >= 3 {
            let parties = tax::list_third_parties(conn)?;
            let hit = parties.into_iter().find(|t| {
                t.kind == kind
                    && (t.name.to_lowercase().contains(&needle)
                        || needle.contains(&t.name.to_lowercase()))
            });
            return Ok(hit);
        }
    }
    Ok(None)
}

fn party_kind_for_doc(doc_type: &str) -> Option<&'static str> {
    match doc_type {
        "supplier_invoice" | "expense_receipt" => Some("supplier"),
        "customer_invoice" => Some("customer"),
        "ticket" => Some("supplier"), // commerçant optionnel
        _ => None,
    }
}

fn build_extraction_payload(
    text: &str,
    method: &str,
    doc_type: &str,
    confidence: f64,
    matched: Option<&ThirdParty>,
) -> serde_json::Value {
    let amounts = parse_amounts(text);
    let date = parse_date(text);
    let number = parse_invoice_number(text);
    let siret = parse_siret(text);
    let party_name = parse_supplier_name(text);
    let reverse = detect_reverse_charge(text) && doc_type == "supplier_invoice";
    let is_sale = doc_type == "customer_invoice";
    let is_bank = doc_type == "bank_statement";
    let vat_mode = if reverse {
        "autoliquidation_sous_traitance"
    } else if is_bank
        || amounts.vat_cents == Some(0)
        || (amounts.vat_cents.is_none()
            && amounts.ht_cents.is_some()
            && amounts.ttc_cents == amounts.ht_cents)
    {
        "sans"
    } else {
        "normale"
    };
    let rate = amounts.rate_bp.unwrap_or(2000);
    let vat_code = match (vat_mode, rate, is_sale) {
        ("autoliquidation_sous_traitance", 1000, _) => "T10R",
        ("autoliquidation_sous_traitance", 550, _) => "T55R",
        ("autoliquidation_sous_traitance", _, _) => "T20R",
        ("sans", _, _) => "TEX",
        (_, 1000, true) => "T10C",
        (_, 550, true) => "T55C",
        (_, _, true) => "T20C",
        (_, 1000, false) => "T10D",
        (_, 550, false) => "T55D",
        _ => "T20D",
    };
    let ht_account = default_ht_account_for(doc_type, reverse);
    let counterpart = default_counterpart_for(doc_type);
    let journal = default_journal_for(doc_type);
    let bank_direction = if is_bank { Some("debit") } else { None };
    let notes = if method == "image_pending_ocr" {
        "Texte peu lisible ou image scannée : OCR local non activé (pas d’API cloud). Vérifiez / saisissez les montants."
    } else if is_bank {
        "Relevé : une écriture BQ brouillon par mouvement saisi (pièce jointe = le relevé). Vérifiez sens, montant et contrepartie (souvent 471 en attente)."
    } else {
        "Vérifiez les champs avant de créer le brouillon d’écriture."
    };

    json!({
        "rawTextPreview": text.chars().take(4000).collect::<String>(),
        "extractionMethod": method,
        "docType": doc_type,
        "confidence": confidence,
        "issueDate": date,
        "invoiceNumber": number,
        "supplierName": party_name,
        "supplierSiret": siret,
        "matchedThirdPartyId": matched.map(|t| t.id),
        "matchedThirdPartyCode": matched.map(|t| t.code.clone()),
        "matchedThirdPartyName": matched.map(|t| t.name.clone()),
        "htCents": amounts.ht_cents.or(amounts.ttc_cents),
        "vatCents": amounts.vat_cents,
        "ttcCents": amounts.ttc_cents,
        "rateBp": amounts.rate_bp,
        "vatMode": vat_mode,
        "vatCode": vat_code,
        "htAccount": ht_account,
        "counterpartAccount": counterpart,
        "journalCode": journal,
        "bankDirection": bank_direction,
        "reverseCharge": reverse,
        "postingSupported": posting_supported(doc_type),
        "notes": notes
    })
}

fn row_to_import(r: &rusqlite::Row<'_>) -> rusqlite::Result<DocumentImport> {
    let extraction_json: String = r.get(6)?;
    let extraction: serde_json::Value =
        serde_json::from_str(&extraction_json).unwrap_or_else(|_| json!({}));
    Ok(DocumentImport {
        id: r.get(0)?,
        doc_type: r.get(1)?,
        source_file_name: r.get(2)?,
        stored_path: r.get(3)?,
        mime_type: r.get(4)?,
        extraction_method: r.get(5)?,
        extraction,
        confidence: r.get(7)?,
        status: r.get(8)?,
        entry_id: r.get(9)?,
        created_at: r.get(10)?,
        confirmed_at: r.get(11)?,
    })
}

const SELECT_COLS: &str = "id, doc_type, source_file_name, stored_path, mime_type,
    extraction_method, extraction_json, confidence, status, entry_id, created_at, confirmed_at";

pub fn get_document_import(conn: &Connection, id: i64) -> DbResult<DocumentImport> {
    conn.query_row(
        &format!("SELECT {SELECT_COLS} FROM document_import WHERE id = ?1"),
        params![id],
        row_to_import,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => "Import document introuvable".into(),
        other => other.into(),
    })
}

pub fn list_document_imports(
    conn: &Connection,
    status: Option<&str>,
) -> DbResult<Vec<DocumentImport>> {
    let mut out = Vec::new();
    if let Some(st) = status {
        let mut stmt = conn.prepare(&format!(
            "SELECT {SELECT_COLS} FROM document_import WHERE status = ?1 ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map(params![st], row_to_import)?;
        for row in rows {
            out.push(row?);
        }
    } else {
        let mut stmt = conn.prepare(&format!(
            "SELECT {SELECT_COLS} FROM document_import ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map([], row_to_import)?;
        for row in rows {
            out.push(row?);
        }
    }
    Ok(out)
}

/// Copie le fichier dans le workspace, extrait, classifie, persiste (statut revue).
pub fn analyze_document(
    conn: &Connection,
    workspace_path: &Path,
    source_path: &Path,
) -> DbResult<DocumentImport> {
    if !source_path.is_file() {
        return Err("Fichier source introuvable".into());
    }
    let file_name = source_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "document.pdf".into());
    let dir = imports_dir(workspace_path);
    fs::create_dir_all(&dir)?;
    let stamp = Utc::now().format("%Y%m%d%H%M%S%.3f");
    let dest = dir.join(format!("{stamp}_{file_name}"));
    fs::copy(source_path, &dest)?;

    let (text, method) = extract_text(&dest);
    let (mut doc_type, mut confidence) = classify_document(&text);
    // Facture ambiguë sans signaux client → privilégier fournisseur (cas le plus courant).
    if doc_type == "unknown" {
        let amounts = parse_amounts(&text);
        let lower = text.to_lowercase();
        if (amounts.ttc_cents.is_some() || amounts.ht_cents.is_some())
            && lower.contains("facture")
            && !lower.contains("à votre attention")
            && !lower.contains("conditions de règlement")
        {
            doc_type = "supplier_invoice".into();
            confidence = confidence.max(0.55);
        }
    }
    let siret = parse_siret(&text);
    let party = parse_supplier_name(&text);
    let kind = party_kind_for_doc(&doc_type).unwrap_or("supplier");
    let matched = match_third_party(conn, kind, party.as_deref(), siret.as_deref())?;
    let payload = build_extraction_payload(
        &text,
        &method,
        &doc_type,
        confidence,
        matched.as_ref(),
    );
    let mime = guess_mime(source_path);
    let now = now_iso();
    conn.execute(
        "INSERT INTO document_import(
            doc_type, source_file_name, stored_path, mime_type,
            extraction_method, extraction_json, confidence, status,
            entry_id, created_at, confirmed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending_review', NULL, ?8, NULL)",
        params![
            doc_type,
            file_name,
            dest.display().to_string(),
            mime,
            method,
            payload.to_string(),
            confidence,
            now
        ],
    )?;
    let id = conn.last_insert_rowid();
    get_document_import(conn, id)
}

pub fn discard_document_import(conn: &Connection, id: i64) -> DbResult<()> {
    let doc = get_document_import(conn, id)?;
    if doc.status == "confirmed" {
        return Err("Import déjà confirmé — ne peut pas être écarté".into());
    }
    conn.execute(
        "UPDATE document_import SET status = 'discarded' WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

fn ensure_party_for_confirm(
    conn: &Connection,
    input: &ConfirmDocumentImportInput,
    kind: &str,
    required: bool,
) -> DbResult<Option<(i64, String)>> {
    if let Some(id) = input.third_party_id {
        let tp = tax::list_third_parties(conn)?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| "Tiers introuvable".to_string())?;
        return Ok(Some((tp.id, tp.code)));
    }
    let name = input
        .create_supplier_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if name.is_none() {
        if required {
            let label = if kind == "customer" {
                "Choisissez un client ou indiquez un nom pour en créer un"
            } else {
                "Choisissez un fournisseur ou indiquez un nom pour en créer un"
            };
            return Err(label.into());
        }
        return Ok(None);
    }
    let name = name.unwrap();
    let prefix = if kind == "customer" { "C" } else { "F" };
    let code = input
        .create_supplier_code
        .clone()
        .filter(|c| !c.trim().is_empty())
        .unwrap_or_else(|| {
            let next = conn
                .query_row(
                    "SELECT COALESCE(MAX(id), 0) + 1 FROM third_party",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(1);
            format!("{prefix}{next}")
        });
    let tp = tax::create_third_party(
        conn,
        &CreateThirdPartyInput {
            code: code.clone(),
            name: name.to_string(),
            kind: kind.into(),
            siret: input.create_supplier_siret.clone(),
            email: None,
            address: None,
            routing_id: None,
        },
    )?;
    Ok(Some((tp.id, tp.code)))
}

fn compute_vat_cents(ht: i64, rate_bp: i32) -> i64 {
    if ht <= 0 || rate_bp <= 0 {
        0
    } else {
        ((ht as i128) * (rate_bp as i128) / 10_000) as i64
    }
}

fn line(
    account: &str,
    label: &str,
    debit: i64,
    credit: i64,
    vat_code: Option<&str>,
    tp: Option<(i64, &str, &str)>,
) -> EntryLineInput {
    EntryLineInput {
        account_number: account.into(),
        label: Some(label.into()),
        debit_cents: debit,
        credit_cents: credit,
        vat_code: vat_code.map(|s| s.into()),
        third_party_id: tp.map(|(id, _, _)| id),
        comp_aux_type: tp.map(|(_, kind, _)| kind.into()),
        comp_aux_num: tp.map(|(_, _, code)| code.into()),
    }
}

/// Construit les lignes ACH selon le régime (classique / sans / autoliquidation BTP).
pub fn build_supplier_invoice_lines(
    ht_cents: i64,
    vat_mode: &str,
    rate_bp: i32,
    vat_code: &str,
    ht_account: &str,
    tp_id: i64,
    tp_code: &str,
) -> Vec<EntryLineInput> {
    let ht = ht_cents.max(0);
    let tp = Some((tp_id, "supplier", tp_code));
    match vat_mode {
        "autoliquidation_sous_traitance" => {
            let vat = compute_vat_cents(ht, rate_bp);
            let mut lines = vec![line(
                ht_account,
                "Charge / sous-traitance",
                ht,
                0,
                Some(vat_code),
                None,
            )];
            if vat > 0 {
                lines.push(line(
                    "44566",
                    "TVA déductible",
                    vat,
                    0,
                    Some(vat_code),
                    None,
                ));
                lines.push(line(
                    "4452",
                    "TVA due autoliquidation",
                    0,
                    vat,
                    Some(vat_code),
                    None,
                ));
            }
            lines.push(line(
                "401",
                &format!("Fournisseur {tp_code}"),
                0,
                ht,
                None,
                tp,
            ));
            lines
        }
        "sans" => {
            vec![
                line(ht_account, "Charge", ht, 0, Some(vat_code), None),
                line(
                    "401",
                    &format!("Fournisseur {tp_code}"),
                    0,
                    ht,
                    None,
                    tp,
                ),
            ]
        }
        _ => {
            let vat = compute_vat_cents(ht, rate_bp);
            let ttc = ht + vat;
            let mut lines = vec![line(ht_account, "Charge", ht, 0, Some(vat_code), None)];
            if vat > 0 {
                lines.push(line(
                    "44566",
                    "TVA déductible",
                    vat,
                    0,
                    Some(vat_code),
                    None,
                ));
            }
            lines.push(line(
                "401",
                &format!("Fournisseur {tp_code}"),
                0,
                ttc,
                None,
                tp,
            ));
            lines
        }
    }
}

/// Facture client → journal VTE (produit + TVA collectée + 411).
pub fn build_customer_invoice_lines(
    ht_cents: i64,
    vat_mode: &str,
    rate_bp: i32,
    vat_code: &str,
    ht_account: &str,
    tp_id: i64,
    tp_code: &str,
) -> Vec<EntryLineInput> {
    let ht = ht_cents.max(0);
    let tp = Some((tp_id, "customer", tp_code));
    match vat_mode {
        "autoliquidation_chantier" | "sans" => {
            vec![
                line(ht_account, "Produit", 0, ht, Some(vat_code), None),
                line(
                    "411",
                    &format!("Client {tp_code}"),
                    ht,
                    0,
                    None,
                    tp,
                ),
            ]
        }
        _ => {
            let vat = compute_vat_cents(ht, rate_bp);
            let ttc = ht + vat;
            let mut lines = vec![line(ht_account, "Produit", 0, ht, Some(vat_code), None)];
            if vat > 0 {
                lines.push(line(
                    "44571",
                    "TVA collectée",
                    0,
                    vat,
                    Some(vat_code),
                    None,
                ));
            }
            lines.push(line(
                "411",
                &format!("Client {tp_code}"),
                ttc,
                0,
                None,
                tp,
            ));
            lines
        }
    }
}

/// Note de frais / ticket → charge + TVA déductible + contrepartie (401 / 512 / 530).
pub fn build_expense_like_lines(
    ht_cents: i64,
    vat_mode: &str,
    rate_bp: i32,
    vat_code: &str,
    ht_account: &str,
    counterpart: &str,
    party: Option<(i64, String)>,
) -> Vec<EntryLineInput> {
    let ht = ht_cents.max(0);
    let vat = if vat_mode == "normale" {
        compute_vat_cents(ht, rate_bp)
    } else {
        0
    };
    let ttc = ht + vat;
    let tp = party.as_ref().map(|(id, code)| {
        (
            *id,
            if counterpart.starts_with("411") {
                "customer"
            } else {
                "supplier"
            },
            code.as_str(),
        )
    });
    let mut lines = vec![line(ht_account, "Charge / frais", ht, 0, Some(vat_code), None)];
    if vat > 0 {
        lines.push(line(
            "44566",
            "TVA déductible",
            vat,
            0,
            Some(vat_code),
            None,
        ));
    }
    let label = match counterpart {
        c if c.starts_with("512") => "Banque".to_string(),
        c if c.starts_with("530") => "Caisse".to_string(),
        c if c.starts_with("401") => party
            .as_ref()
            .map(|(_, code)| format!("Fournisseur {code}"))
            .unwrap_or_else(|| "Fournisseur".into()),
        _ => format!("Contrepartie {counterpart}"),
    };
    lines.push(line(counterpart, &label, 0, ttc, None, tp));
    lines
}

/// Relevé bancaire → une écriture BQ (512 ↔ contrepartie, souvent 471).
/// `bank_direction`: `debit` = sortie (crédit 512), `credit` = entrée (débit 512).
pub fn build_bank_statement_lines(
    amount_cents: i64,
    bank_account: &str,
    counterpart: &str,
    direction: &str,
    party: Option<(i64, String)>,
) -> Vec<EntryLineInput> {
    let amount = amount_cents.max(0);
    let bank = if bank_account.trim().is_empty() {
        "512"
    } else {
        bank_account
    };
    let other = if counterpart.trim().is_empty() {
        "471"
    } else {
        counterpart
    };
    let tp = party.as_ref().map(|(id, code)| {
        let kind = if other.starts_with("411") {
            "customer"
        } else if other.starts_with("401") {
            "supplier"
        } else {
            "supplier"
        };
        (*id, kind, code.as_str())
    });
    let other_label = format!("Contrepartie {other}");
    if direction == "credit" {
        // Entrée d'argent
        vec![
            line(bank, "Banque", amount, 0, None, None),
            line(other, &other_label, 0, amount, None, tp),
        ]
    } else {
        // Sortie d'argent (défaut)
        vec![
            line(other, &other_label, amount, 0, None, tp),
            line(bank, "Banque", 0, amount, None, None),
        ]
    }
}

fn journal_for_expense_counterpart(counterpart: &str, fallback: &str) -> String {
    if counterpart.starts_with("401") {
        "ACH".into()
    } else if counterpart.starts_with("512") || counterpart.starts_with("530") {
        "OD".into()
    } else {
        fallback.into()
    }
}

/// Confirmation humaine obligatoire → brouillon d’écriture + pièce jointe liée.
pub fn confirm_document_import(
    conn: &Connection,
    id: i64,
    input: &ConfirmDocumentImportInput,
) -> DbResult<DocumentImport> {
    let doc = get_document_import(conn, id)?;
    if doc.status != "pending_review" {
        return Err("Seuls les imports en revue peuvent être confirmés".into());
    }
    let doc_type = if input.doc_type.trim().is_empty() {
        doc.doc_type.clone()
    } else {
        input.doc_type.trim().to_string()
    };
    if !DOC_TYPES.contains(&doc_type.as_str()) {
        return Err("Type de document invalide".into());
    }
    if !posting_supported(&doc_type) {
        return Err(
            "Précisez le type de document (inconnu ne crée pas d’écriture).".into(),
        );
    }
    let ht = input.ht_cents;
    if ht <= 0 {
        return Err("Montant invalide (HT ou montant du mouvement)".into());
    }
    let vat_mode = input.vat_mode.as_str();
    let rate = input.rate_bp.max(0);
    let default_vat = match doc_type.as_str() {
        "customer_invoice" => "T20C",
        "bank_statement" => "TEX",
        _ => "T20D",
    };
    let vat_code = if input.vat_code.trim().is_empty() {
        default_vat.to_string()
    } else {
        input.vat_code.trim().to_string()
    };
    tax::ensure_vat_code(conn, &vat_code)?;

    let counterpart = input
        .counterpart_account
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_counterpart_for(&doc_type))
        .to_string();

    let ht_account = if input.ht_account.trim().is_empty() {
        default_ht_account_for(
            &doc_type,
            vat_mode == "autoliquidation_sous_traitance",
        )
        .to_string()
    } else {
        input.ht_account.trim().to_string()
    };

    let (lines, journal) = match doc_type.as_str() {
        "supplier_invoice" => {
            if !matches!(
                vat_mode,
                "normale" | "sans" | "autoliquidation_sous_traitance"
            ) {
                return Err("Régime TVA invalide pour une facture fournisseur".into());
            }
            let (tp_id, tp_code) = ensure_party_for_confirm(conn, input, "supplier", true)?
                .ok_or_else(|| "Fournisseur requis".to_string())?;
            let lines = if let Some(v) = input.vat_cents.filter(|v| *v >= 0) {
                if vat_mode == "normale" {
                    let ttc = ht + v;
                    let mut lines =
                        vec![line(&ht_account, "Charge", ht, 0, Some(&vat_code), None)];
                    if v > 0 {
                        lines.push(line(
                            "44566",
                            "TVA déductible",
                            v,
                            0,
                            Some(&vat_code),
                            None,
                        ));
                    }
                    lines.push(line(
                        "401",
                        &format!("Fournisseur {tp_code}"),
                        0,
                        ttc,
                        None,
                        Some((tp_id, "supplier", &tp_code)),
                    ));
                    lines
                } else {
                    build_supplier_invoice_lines(
                        ht, vat_mode, rate, &vat_code, &ht_account, tp_id, &tp_code,
                    )
                }
            } else {
                build_supplier_invoice_lines(
                    ht, vat_mode, rate, &vat_code, &ht_account, tp_id, &tp_code,
                )
            };
            let journal = input.journal_code.trim().to_string().if_empty("ACH");
            (lines, journal)
        }
        "customer_invoice" => {
            if !matches!(
                vat_mode,
                "normale" | "sans" | "autoliquidation_chantier"
            ) {
                return Err("Régime TVA invalide pour une facture client".into());
            }
            let (tp_id, tp_code) = ensure_party_for_confirm(conn, input, "customer", true)?
                .ok_or_else(|| "Client requis".to_string())?;
            let lines = if let Some(v) = input.vat_cents.filter(|v| *v >= 0) {
                if vat_mode == "normale" {
                    let ttc = ht + v;
                    let mut lines =
                        vec![line(&ht_account, "Produit", 0, ht, Some(&vat_code), None)];
                    if v > 0 {
                        lines.push(line(
                            "44571",
                            "TVA collectée",
                            0,
                            v,
                            Some(&vat_code),
                            None,
                        ));
                    }
                    lines.push(line(
                        "411",
                        &format!("Client {tp_code}"),
                        ttc,
                        0,
                        None,
                        Some((tp_id, "customer", &tp_code)),
                    ));
                    lines
                } else {
                    build_customer_invoice_lines(
                        ht, vat_mode, rate, &vat_code, &ht_account, tp_id, &tp_code,
                    )
                }
            } else {
                build_customer_invoice_lines(
                    ht, vat_mode, rate, &vat_code, &ht_account, tp_id, &tp_code,
                )
            };
            let journal = input.journal_code.trim().to_string().if_empty("VTE");
            (lines, journal)
        }
        "expense_receipt" | "ticket" => {
            if !matches!(vat_mode, "normale" | "sans") {
                return Err("Régime TVA invalide pour un frais / ticket (normale ou sans)".into());
            }
            let need_supplier = counterpart.starts_with("401");
            let party = ensure_party_for_confirm(conn, input, "supplier", need_supplier)?;
            let lines = if let Some(v) = input.vat_cents.filter(|v| *v > 0) {
                if vat_mode == "normale" {
                    let ttc = ht + v;
                    let mut lines = vec![line(
                        &ht_account,
                        "Charge / frais",
                        ht,
                        0,
                        Some(&vat_code),
                        None,
                    )];
                    lines.push(line(
                        "44566",
                        "TVA déductible",
                        v,
                        0,
                        Some(&vat_code),
                        None,
                    ));
                    let tp = party.as_ref().map(|(id, code)| (*id, "supplier", code.as_str()));
                    let label = if counterpart.starts_with("512") {
                        "Banque".into()
                    } else if counterpart.starts_with("530") {
                        "Caisse".into()
                    } else {
                        party
                            .as_ref()
                            .map(|(_, c)| format!("Fournisseur {c}"))
                            .unwrap_or_else(|| "Contrepartie".into())
                    };
                    lines.push(line(&counterpart, &label, 0, ttc, None, tp));
                    lines
                } else {
                    build_expense_like_lines(
                        ht,
                        vat_mode,
                        rate,
                        &vat_code,
                        &ht_account,
                        &counterpart,
                        party.clone(),
                    )
                }
            } else {
                build_expense_like_lines(
                    ht,
                    vat_mode,
                    rate,
                    &vat_code,
                    &ht_account,
                    &counterpart,
                    party,
                )
            };
            let fallback = default_journal_for(&doc_type);
            let journal = if input.journal_code.trim().is_empty() {
                journal_for_expense_counterpart(&counterpart, fallback)
            } else {
                input.journal_code.trim().to_string()
            };
            (lines, journal)
        }
        "bank_statement" => {
            let direction = input
                .bank_direction
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("debit");
            if direction != "debit" && direction != "credit" {
                return Err("Sens bancaire invalide (debit = sortie, credit = entrée)".into());
            }
            // Pour BQ : ht_account = contrepartie (471…), counterpart_account = 512
            let bank_account = if counterpart.starts_with("512") {
                counterpart.clone()
            } else {
                "512".into()
            };
            let other = if ht_account.starts_with("512") {
                "471".into()
            } else {
                ht_account.clone()
            };
            let party = ensure_party_for_confirm(conn, input, "supplier", false)?;
            let lines =
                build_bank_statement_lines(ht, &bank_account, &other, direction, party);
            let journal = input.journal_code.trim().to_string().if_empty("BQ");
            (lines, journal)
        }
        _ => return Err("Type de document non supporté pour le posting".into()),
    };

    let label = if input.label.trim().is_empty() {
        let piece = input.piece_ref.trim().to_string().if_empty("document");
        format!("Import {piece}")
    } else {
        input.label.trim().to_string()
    };
    let entry = ledger::create_draft(
        conn,
        &CreateEntryInput {
            journal_code: journal.clone(),
            entry_date: input.entry_date.clone(),
            piece_ref: input.piece_ref.clone(),
            label,
            lines,
        },
    )?;
    let mime = doc.mime_type.clone();
    ledger::add_attachment(
        conn,
        entry.id,
        &doc.source_file_name,
        &doc.stored_path,
        mime,
    )?;

    let mut extraction = doc.extraction;
    if let Some(obj) = extraction.as_object_mut() {
        obj.insert("confirmedDocType".into(), json!(doc_type));
        obj.insert("confirmedHtCents".into(), json!(ht));
        obj.insert("confirmedVatMode".into(), json!(vat_mode));
        obj.insert("confirmedJournal".into(), json!(journal));
        obj.insert("confirmedEntryId".into(), json!(entry.id));
    }
    let now = now_iso();
    conn.execute(
        "UPDATE document_import SET
            doc_type = ?1,
            extraction_json = ?2,
            status = 'confirmed',
            entry_id = ?3,
            confirmed_at = ?4
         WHERE id = ?5",
        params![doc_type, extraction.to_string(), entry.id, now, id],
    )?;
    get_document_import(conn, id)
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, open_connection};
    use printpdf::{Mm, PdfDocument};
    use std::io::BufWriter;
    use tempfile::tempdir;

    fn sample_invoice_text() -> &'static str {
        r#"
FOURNITURES DELTA SAS
12 rue de l'Industrie
SIRET 12345678900012

FACTURE N° FA-2026-0042
Date : 15/03/2026

Désignation                  HT
Fournitures bureau        100,00

Total HT                  100,00
TVA 20 %                   20,00
Total TTC                 120,00
Net à payer               120,00 €
"#
    }

    #[test]
    fn classify_supplier_invoice_high_confidence() {
        let (t, c) = classify_document(sample_invoice_text());
        assert_eq!(t, "supplier_invoice");
        assert!(c >= 0.45, "confidence {c}");
    }

    #[test]
    fn classify_customer_invoice() {
        let text = r#"
FACTURE N° FAC-88
À votre attention
Client : ACME SAS
Conditions de règlement : 30 jours
Total HT 500,00
TVA 20 % 100,00
Total TTC 600,00
Merci de votre confiance
"#;
        let (t, c) = classify_document(text);
        assert_eq!(t, "customer_invoice");
        assert!(c >= 0.45, "confidence {c}");
    }

    #[test]
    fn classify_expense_and_ticket_and_bank() {
        let (t, _) = classify_document(
            "NOTE DE FRAIS — Restaurant Le Midi — Total TTC 45,00 — Justificatif",
        );
        assert_eq!(t, "expense_receipt");
        let (t, _) = classify_document(
            "TICKET DE CAISSE\nCarte bancaire CB ****1234\nMerci de votre visite\nTotal 12,50",
        );
        assert_eq!(t, "ticket");
        let (t, _) = classify_document(
            "RELEVÉ DE COMPTE\nIBAN FR76 1234\nSolde précédent 1000,00\nNouveau solde 850,00",
        );
        assert_eq!(t, "bank_statement");
    }

    #[test]
    fn build_customer_and_expense_and_bank_balanced() {
        let lines = build_customer_invoice_lines(
            10_000, "normale", 2000, "T20C", "706", 2, "C001",
        );
        let d: i64 = lines.iter().map(|l| l.debit_cents).sum();
        let c: i64 = lines.iter().map(|l| l.credit_cents).sum();
        assert_eq!(d, c);
        assert_eq!(d, 12_000);
        assert!(lines.iter().any(|l| l.account_number == "44571"));

        let lines = build_expense_like_lines(
            5_000,
            "normale",
            2000,
            "T20D",
            "625",
            "530",
            None,
        );
        let d: i64 = lines.iter().map(|l| l.debit_cents).sum();
        let c: i64 = lines.iter().map(|l| l.credit_cents).sum();
        assert_eq!(d, c);
        assert!(lines.iter().any(|l| l.account_number == "530"));

        let lines = build_bank_statement_lines(15_000, "512", "471", "debit", None);
        let d: i64 = lines.iter().map(|l| l.debit_cents).sum();
        let c: i64 = lines.iter().map(|l| l.credit_cents).sum();
        assert_eq!(d, c);
        assert_eq!(d, 15_000);
    }

    #[test]
    fn parse_amounts_fr() {
        let a = parse_amounts(sample_invoice_text());
        assert_eq!(a.ht_cents, Some(10_000));
        assert_eq!(a.vat_cents, Some(2_000));
        assert_eq!(a.ttc_cents, Some(12_000));
        assert_eq!(a.rate_bp, Some(2000));
    }

    #[test]
    fn parse_date_and_number() {
        assert_eq!(
            parse_date(sample_invoice_text()).as_deref(),
            Some("2026-03-15")
        );
        assert_eq!(
            parse_invoice_number(sample_invoice_text()).as_deref(),
            Some("FA-2026-0042")
        );
    }

    #[test]
    fn reverse_charge_detected() {
        let text = "Facture sous-traitance — Autoliquidation art. 283 CGI — Total HT 1000,00";
        assert!(detect_reverse_charge(text));
        let (t, _) = classify_document(&format!("{text}\nTVA due par le preneur"));
        assert_eq!(t, "supplier_invoice");
    }

    #[test]
    fn build_lines_normale_balanced() {
        let lines = build_supplier_invoice_lines(
            10_000, "normale", 2000, "T20D", "606", 1, "F001",
        );
        let d: i64 = lines.iter().map(|l| l.debit_cents).sum();
        let c: i64 = lines.iter().map(|l| l.credit_cents).sum();
        assert_eq!(d, c);
        assert_eq!(d, 12_000);
    }

    #[test]
    fn build_lines_reverse_balanced() {
        let lines = build_supplier_invoice_lines(
            10_000,
            "autoliquidation_sous_traitance",
            2000,
            "T20R",
            "604",
            1,
            "F001",
        );
        let d: i64 = lines.iter().map(|l| l.debit_cents).sum();
        let c: i64 = lines.iter().map(|l| l.credit_cents).sum();
        assert_eq!(d, c);
        assert_eq!(d, 12_000); // 100 HT + 20 TVA déductible
    }

    fn write_simple_pdf(path: &Path, text: &str) {
        let (doc, page, layer) = PdfDocument::new("Test", Mm(210.0), Mm(297.0), "Layer");
        let font = doc.add_builtin_font(printpdf::BuiltinFont::Helvetica).unwrap();
        let layer = doc.get_page(page).get_layer(layer);
        let mut y = 280.0;
        for line in text.lines() {
            if line.trim().is_empty() {
                y -= 5.0;
                continue;
            }
            layer.use_text(line, 10.0, Mm(12.0), Mm(y), &font);
            y -= 6.0;
            if y < 20.0 {
                break;
            }
        }
        let file = fs::File::create(path).unwrap();
        doc.save(&mut BufWriter::new(file)).unwrap();
    }

    #[test]
    fn analyze_confirm_creates_draft_with_attachment() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) =
            db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("fa-delta.pdf");
        write_simple_pdf(&pdf, sample_invoice_text());

        let doc = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        assert_eq!(doc.status, "pending_review");
        assert!(Path::new(&doc.stored_path).is_file());
        // Extraction may vary on pdf-extract quality; force confirm with known amounts.
        let confirmed = confirm_document_import(
            &conn,
            doc.id,
            &ConfirmDocumentImportInput {
                doc_type: "supplier_invoice".into(),
                entry_date: "2026-03-15".into(),
                piece_ref: "FA-2026-0042".into(),
                label: "Import FA-2026-0042 Delta".into(),
                journal_code: "ACH".into(),
                ht_cents: 10_000,
                vat_cents: Some(2_000),
                rate_bp: 2000,
                vat_mode: "normale".into(),
                vat_code: "T20D".into(),
                ht_account: "606".into(),
                third_party_id: None,
                counterpart_account: None,
                bank_direction: None,
                create_supplier_name: Some("FOURNITURES DELTA SAS".into()),
                create_supplier_code: Some("F900".into()),
                create_supplier_siret: Some("12345678900012".into()),
            },
        )
        .unwrap();
        assert_eq!(confirmed.status, "confirmed");
        let entry_id = confirmed.entry_id.expect("entry");
        let entry = ledger::get_entry(&conn, entry_id).unwrap();
        assert_eq!(entry.status, "draft");
        assert_eq!(entry.journal_code, "ACH");
        assert_eq!(entry.attachments.len(), 1);
        let d: i64 = entry.lines.iter().map(|l| l.debit_cents).sum();
        let c: i64 = entry.lines.iter().map(|l| l.credit_cents).sum();
        assert_eq!(d, c);
        assert_eq!(d, 12_000);
    }

    #[test]
    fn confirm_customer_invoice_creates_vte_draft() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("fa-client.pdf");
        write_simple_pdf(
            &pdf,
            "FACTURE N° FAC-1\nÀ votre attention\nClient ACME\nTotal HT 100,00\nTVA 20,00\nTotal TTC 120,00\nConditions de règlement",
        );
        let doc = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        let confirmed = confirm_document_import(
            &conn,
            doc.id,
            &ConfirmDocumentImportInput {
                doc_type: "customer_invoice".into(),
                entry_date: "2026-03-15".into(),
                piece_ref: "FAC-1".into(),
                label: "Import FAC-1".into(),
                journal_code: "VTE".into(),
                ht_cents: 10_000,
                vat_cents: Some(2_000),
                rate_bp: 2000,
                vat_mode: "normale".into(),
                vat_code: "T20C".into(),
                ht_account: "706".into(),
                counterpart_account: Some("411".into()),
                bank_direction: None,
                third_party_id: None,
                create_supplier_name: Some("ACME SAS".into()),
                create_supplier_code: Some("C900".into()),
                create_supplier_siret: None,
            },
        )
        .unwrap();
        let entry = ledger::get_entry(&conn, confirmed.entry_id.unwrap()).unwrap();
        assert_eq!(entry.journal_code, "VTE");
        assert!(entry.lines.iter().any(|l| l.account_number == "411"));
        assert_eq!(entry.attachments.len(), 1);
    }

    #[test]
    fn confirm_ticket_creates_od_draft() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("ticket.pdf");
        write_simple_pdf(&pdf, "TICKET DE CAISSE\nCB ****42\nTotal TTC 25,00\nMerci");
        let doc = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        let confirmed = confirm_document_import(
            &conn,
            doc.id,
            &ConfirmDocumentImportInput {
                doc_type: "ticket".into(),
                entry_date: "2026-03-15".into(),
                piece_ref: "TK-1".into(),
                label: "Ticket caisse".into(),
                journal_code: "OD".into(),
                ht_cents: 2_083,
                vat_cents: Some(417),
                rate_bp: 2000,
                vat_mode: "normale".into(),
                vat_code: "T20D".into(),
                ht_account: "625".into(),
                counterpart_account: Some("530".into()),
                bank_direction: None,
                third_party_id: None,
                create_supplier_name: None,
                create_supplier_code: None,
                create_supplier_siret: None,
            },
        )
        .unwrap();
        let entry = ledger::get_entry(&conn, confirmed.entry_id.unwrap()).unwrap();
        assert_eq!(entry.journal_code, "OD");
        assert!(entry.lines.iter().any(|l| l.account_number == "530"));
    }

    #[test]
    fn schema_version_includes_documents() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.sascompta");
        let conn = open_connection(&path).unwrap();
        db::migrate(&conn).unwrap();
        let version: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, db::SCHEMA_VERSION.to_string());
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'document_import'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }
}
