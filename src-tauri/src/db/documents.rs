//! Import de documents (lot 1 : PDF facture fournisseur → brouillon ACH/TVA).
//!
//! Architecture extensible : classifiers typés + modèles de lignes par `doc_type`.
//! Les types futurs (tickets, relevés, notes de frais, factures clients) branchent
//! un classifier + un template sans refondre le flux analyse → revue → confirmation.

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

/// Types pour lesquels un modèle d’écriture est implémenté (lot 1).
pub fn posting_supported(doc_type: &str) -> bool {
    doc_type == "supplier_invoice"
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
                    v += 0.35;
                }
                if lower.contains("fournisseur") || lower.contains("à payer") || lower.contains("a payer")
                {
                    v += 0.15;
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
                    v += 0.1;
                }
                // Customer invoice cues reduce supplier score slightly handled elsewhere
                v.min(0.98)
            }
            "customer_invoice" => {
                let mut v: f64 = 0.0;
                if lower.contains("facture") {
                    v += 0.2;
                }
                if lower.contains("client") || lower.contains("à votre attention") {
                    v += 0.25;
                }
                if lower.contains("conditions de règlement") {
                    v += 0.1;
                }
                v.min(0.85)
            }
            "expense_receipt" => {
                let mut v: f64 = 0.0;
                if lower.contains("reçu") || lower.contains("ticket de caisse") {
                    v += 0.4;
                }
                if lower.contains("merci de votre visite") {
                    v += 0.2;
                }
                v.min(0.8)
            }
            "bank_statement" => {
                let mut v: f64 = 0.0;
                if lower.contains("relevé") && lower.contains("compte") {
                    v += 0.5;
                }
                if lower.contains("iban") && lower.contains("solde") {
                    v += 0.3;
                }
                v.min(0.9)
            }
            "ticket" => {
                let mut v: f64 = 0.0;
                if lower.contains("ticket") {
                    v += 0.35;
                }
                if lower.contains("cb ") || lower.contains("carte bancaire") {
                    v += 0.15;
                }
                v.min(0.75)
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

fn match_supplier(
    conn: &Connection,
    name: Option<&str>,
    siret: Option<&str>,
) -> DbResult<Option<ThirdParty>> {
    if let Some(s) = siret {
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 9 {
            let found = conn
                .query_row(
                    "SELECT id FROM third_party WHERE kind = 'supplier'
                     AND replace(coalesce(siret,''), ' ', '') LIKE ?1 || '%'
                     LIMIT 1",
                    params![digits],
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
                t.kind == "supplier" && t.name.to_lowercase().contains(&needle)
                    || (t.kind == "supplier" && needle.contains(&t.name.to_lowercase()))
            });
            return Ok(hit);
        }
    }
    Ok(None)
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
    let supplier = parse_supplier_name(text);
    let reverse = detect_reverse_charge(text);
    let vat_mode = if reverse {
        "autoliquidation_sous_traitance"
    } else if amounts.vat_cents == Some(0) || (amounts.vat_cents.is_none() && amounts.ht_cents.is_some() && amounts.ttc_cents == amounts.ht_cents)
    {
        "sans"
    } else {
        "normale"
    };
    let rate = amounts.rate_bp.unwrap_or(2000);
    let vat_code = match (vat_mode, rate) {
        ("autoliquidation_sous_traitance", 1000) => "T10R",
        ("autoliquidation_sous_traitance", 550) => "T55R",
        ("autoliquidation_sous_traitance", _) => "T20R",
        ("sans", _) => "TEX",
        (_, 1000) => "T10D",
        (_, 550) => "T55D",
        _ => "T20D",
    };
    let ht_account = if reverse { "604" } else { "606" };

    json!({
        "rawTextPreview": text.chars().take(4000).collect::<String>(),
        "extractionMethod": method,
        "docType": doc_type,
        "confidence": confidence,
        "issueDate": date,
        "invoiceNumber": number,
        "supplierName": supplier,
        "supplierSiret": siret,
        "matchedThirdPartyId": matched.map(|t| t.id),
        "matchedThirdPartyCode": matched.map(|t| t.code.clone()),
        "matchedThirdPartyName": matched.map(|t| t.name.clone()),
        "htCents": amounts.ht_cents,
        "vatCents": amounts.vat_cents,
        "ttcCents": amounts.ttc_cents,
        "rateBp": amounts.rate_bp,
        "vatMode": vat_mode,
        "vatCode": vat_code,
        "htAccount": ht_account,
        "journalCode": if posting_supported(doc_type) { "ACH" } else { "" },
        "reverseCharge": reverse,
        "postingSupported": posting_supported(doc_type),
        "notes": if method == "image_pending_ocr" {
            "Texte peu lisible ou image scannée : OCR local non activé (pas d’API cloud). Vérifiez / saisissez les montants."
        } else {
            "Vérifiez les champs avant de créer le brouillon d’écriture."
        }
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
    // Lot 1 biais : PDF facture avec montants → privilégier fournisseur si ambigü.
    if doc_type == "customer_invoice" || doc_type == "unknown" {
        let amounts = parse_amounts(&text);
        if amounts.ttc_cents.is_some() || amounts.ht_cents.is_some() {
            if text.to_lowercase().contains("facture") {
                doc_type = "supplier_invoice".into();
                confidence = confidence.max(0.55);
            }
        }
    }
    let siret = parse_siret(&text);
    let supplier = parse_supplier_name(&text);
    let matched = match_supplier(conn, supplier.as_deref(), siret.as_deref())?;
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

fn ensure_supplier_for_confirm(
    conn: &Connection,
    input: &ConfirmDocumentImportInput,
) -> DbResult<(i64, String)> {
    if let Some(id) = input.third_party_id {
        let tp = tax::list_third_parties(conn)?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| "Tiers introuvable".to_string())?;
        return Ok((tp.id, tp.code));
    }
    let name = input
        .create_supplier_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "Choisissez un fournisseur ou indiquez un nom pour en créer un".to_string()
        })?;
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
            format!("F{next}")
        });
    let tp = tax::create_third_party(
        conn,
        &CreateThirdPartyInput {
            code: code.clone(),
            name: name.to_string(),
            kind: "supplier".into(),
            siret: input.create_supplier_siret.clone(),
            email: None,
            address: None,
            routing_id: None,
        },
    )?;
    Ok((tp.id, tp.code))
}

fn compute_vat_cents(ht: i64, rate_bp: i32) -> i64 {
    if ht <= 0 || rate_bp <= 0 {
        0
    } else {
        ((ht as i128) * (rate_bp as i128) / 10_000) as i64
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
    match vat_mode {
        "autoliquidation_sous_traitance" => {
            let vat = compute_vat_cents(ht, rate_bp);
            let mut lines = vec![EntryLineInput {
                account_number: ht_account.into(),
                label: Some("Charge / sous-traitance".into()),
                debit_cents: ht,
                credit_cents: 0,
                vat_code: Some(vat_code.into()),
                third_party_id: None,
                comp_aux_type: None,
                comp_aux_num: None,
            }];
            if vat > 0 {
                lines.push(EntryLineInput {
                    account_number: "44566".into(),
                    label: Some("TVA déductible".into()),
                    debit_cents: vat,
                    credit_cents: 0,
                    vat_code: Some(vat_code.into()),
                    third_party_id: None,
                    comp_aux_type: None,
                    comp_aux_num: None,
                });
                lines.push(EntryLineInput {
                    account_number: "4452".into(),
                    label: Some("TVA due autoliquidation".into()),
                    debit_cents: 0,
                    credit_cents: vat,
                    vat_code: Some(vat_code.into()),
                    third_party_id: None,
                    comp_aux_type: None,
                    comp_aux_num: None,
                });
            }
            lines.push(EntryLineInput {
                account_number: "401".into(),
                label: Some(format!("Fournisseur {tp_code}")),
                debit_cents: 0,
                credit_cents: ht,
                vat_code: None,
                third_party_id: Some(tp_id),
                comp_aux_type: Some("supplier".into()),
                comp_aux_num: Some(tp_code.into()),
            });
            lines
        }
        "sans" => {
            vec![
                EntryLineInput {
                    account_number: ht_account.into(),
                    label: Some("Charge".into()),
                    debit_cents: ht,
                    credit_cents: 0,
                    vat_code: Some(vat_code.into()),
                    third_party_id: None,
                    comp_aux_type: None,
                    comp_aux_num: None,
                },
                EntryLineInput {
                    account_number: "401".into(),
                    label: Some(format!("Fournisseur {tp_code}")),
                    debit_cents: 0,
                    credit_cents: ht,
                    vat_code: None,
                    third_party_id: Some(tp_id),
                    comp_aux_type: Some("supplier".into()),
                    comp_aux_num: Some(tp_code.into()),
                },
            ]
        }
        _ => {
            // normale
            let vat = compute_vat_cents(ht, rate_bp);
            let ttc = ht + vat;
            let mut lines = vec![EntryLineInput {
                account_number: ht_account.into(),
                label: Some("Charge".into()),
                debit_cents: ht,
                credit_cents: 0,
                vat_code: Some(vat_code.into()),
                third_party_id: None,
                comp_aux_type: None,
                comp_aux_num: None,
            }];
            if vat > 0 {
                lines.push(EntryLineInput {
                    account_number: "44566".into(),
                    label: Some("TVA déductible".into()),
                    debit_cents: vat,
                    credit_cents: 0,
                    vat_code: Some(vat_code.into()),
                    third_party_id: None,
                    comp_aux_type: None,
                    comp_aux_num: None,
                });
            }
            lines.push(EntryLineInput {
                account_number: "401".into(),
                label: Some(format!("Fournisseur {tp_code}")),
                debit_cents: 0,
                credit_cents: ttc,
                vat_code: None,
                third_party_id: Some(tp_id),
                comp_aux_type: Some("supplier".into()),
                comp_aux_num: Some(tp_code.into()),
            });
            lines
        }
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
        return Err(format!(
            "Le type « {doc_type} » n’a pas encore de modèle d’écriture (lot 1 = facture fournisseur). Changez le type ou attendez l’extension."
        )
        .into());
    }
    let ht = input.ht_cents;
    if ht <= 0 {
        return Err("Montant HT invalide".into());
    }
    let vat_mode = input.vat_mode.as_str();
    if !matches!(
        vat_mode,
        "normale" | "sans" | "autoliquidation_sous_traitance"
    ) {
        return Err("Régime TVA invalide pour une facture fournisseur".into());
    }
    let (tp_id, tp_code) = ensure_supplier_for_confirm(conn, input)?;
    let rate = input.rate_bp.max(0);
    let vat_code = if input.vat_code.trim().is_empty() {
        "T20D".to_string()
    } else {
        input.vat_code.trim().to_string()
    };
    tax::ensure_vat_code(conn, &vat_code)?;
    let ht_account = if input.ht_account.trim().is_empty() {
        if vat_mode == "autoliquidation_sous_traitance" {
            "604".into()
        } else {
            "606".into()
        }
    } else {
        input.ht_account.trim().to_string()
    };
    let lines = if let Some(v) = input.vat_cents {
        // Respect montant TVA saisi en revue pour le mode normale
        if vat_mode == "normale" {
            let ttc = ht + v.max(0);
            let mut lines = vec![EntryLineInput {
                account_number: ht_account.clone(),
                label: Some("Charge".into()),
                debit_cents: ht,
                credit_cents: 0,
                vat_code: Some(vat_code.clone()),
                third_party_id: None,
                comp_aux_type: None,
                comp_aux_num: None,
            }];
            if v > 0 {
                lines.push(EntryLineInput {
                    account_number: "44566".into(),
                    label: Some("TVA déductible".into()),
                    debit_cents: v,
                    credit_cents: 0,
                    vat_code: Some(vat_code.clone()),
                    third_party_id: None,
                    comp_aux_type: None,
                    comp_aux_num: None,
                });
            }
            lines.push(EntryLineInput {
                account_number: "401".into(),
                label: Some(format!("Fournisseur {tp_code}")),
                debit_cents: 0,
                credit_cents: ttc,
                vat_code: None,
                third_party_id: Some(tp_id),
                comp_aux_type: Some("supplier".into()),
                comp_aux_num: Some(tp_code.clone()),
            });
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

    let journal = if input.journal_code.trim().is_empty() {
        "ACH".to_string()
    } else {
        input.journal_code.trim().to_string()
    };
    let label = if input.label.trim().is_empty() {
        format!(
            "Import facture {}",
            input
                .piece_ref
                .trim()
                .to_string()
                .if_empty("fournisseur")
        )
    } else {
        input.label.trim().to_string()
    };
    let entry = ledger::create_draft(
        conn,
        &CreateEntryInput {
            journal_code: journal,
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

    // Merge review into extraction_json for audit trail
    let mut extraction = doc.extraction;
    if let Some(obj) = extraction.as_object_mut() {
        obj.insert("confirmedDocType".into(), json!(doc_type));
        obj.insert("confirmedHtCents".into(), json!(ht));
        obj.insert("confirmedVatMode".into(), json!(vat_mode));
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
