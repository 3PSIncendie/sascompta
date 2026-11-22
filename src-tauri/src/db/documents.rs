//! Import de documents → brouillon d’écriture (revue humaine obligatoire).
//!
//! Types pris en charge : facture fournisseur (ACH), facture client (VTE),
//! devis client (Commercial, sans écriture), note de frais / reçu (ACH ou OD),
//! ticket de caisse (OD/ACH), relevé bancaire (BQ).
//! Flux commun : analyse locale → revue → confirmation → `create_draft` + pièce
//! jointe → `post_entry` (écriture immédiatement visible au Grand livre / TVA / FEC).
//! Facture client : crée aussi une fiche Factures émise (numéro externe) pour Encaisser.
//! Devis client : crée une fiche Commercial → Devis (numéro externe) — pas de VTE.

use super::billing;
use super::ledger;
use super::tax;
use super::DbResult;
use crate::models::*;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub const DOC_TYPES: &[&str] = &[
    "supplier_invoice",
    "customer_invoice",
    "customer_quote",
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

/// Types confirmables : écriture journal **ou** fiche commerciale (devis).
pub fn confirm_supported(doc_type: &str) -> bool {
    posting_supported(doc_type) || doc_type == "customer_quote"
}

pub fn default_journal_for(doc_type: &str) -> &'static str {
    match doc_type {
        "supplier_invoice" => "ACH",
        "customer_invoice" => "VTE",
        "customer_quote" => "DEV",
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
        "customer_invoice" | "customer_quote" => "706",
        "expense_receipt" | "ticket" => "625",
        "bank_statement" => "471",
        _ => "606",
    }
}

pub fn default_counterpart_for(doc_type: &str) -> &'static str {
    match doc_type {
        "supplier_invoice" | "expense_receipt" => "401",
        "customer_invoice" | "customer_quote" => "411",
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
                    'supplier_invoice','customer_invoice','customer_quote',
                    'expense_receipt','bank_statement','ticket','unknown'
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
            invoice_id INTEGER,
            quote_id INTEGER,
            created_at TEXT NOT NULL,
            confirmed_at TEXT,
            source_fingerprint TEXT,
            source_origin_path TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_document_import_status
            ON document_import(status);
        "#,
    )?;
    // Workspaces créés avant v14 / v15 : colonnes idempotence + lien facture.
    let mut stmt = conn.prepare("PRAGMA table_info(document_import)")?;
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    if !cols.iter().any(|c| c == "source_fingerprint") {
        conn.execute(
            "ALTER TABLE document_import ADD COLUMN source_fingerprint TEXT",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "source_origin_path") {
        conn.execute(
            "ALTER TABLE document_import ADD COLUMN source_origin_path TEXT",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "invoice_id") {
        conn.execute(
            "ALTER TABLE document_import ADD COLUMN invoice_id INTEGER",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "quote_id") {
        conn.execute(
            "ALTER TABLE document_import ADD COLUMN quote_id INTEGER",
            [],
        )?;
    }
    // v19 — élargir le CHECK doc_type pour accepter customer_quote (rebuild).
    enrich_document_import_quote_type(conn)?;
    // Un même fichier (contenu) n’est importé qu’une fois tant qu’il n’est pas écarté.
    conn.execute_batch(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_document_import_fingerprint_active
            ON document_import(source_fingerprint)
            WHERE source_fingerprint IS NOT NULL AND status != 'discarded';
        "#,
    )?;
    Ok(())
}

/// Rebuild `document_import` si le CHECK historique n’autorise pas `customer_quote`.
fn enrich_document_import_quote_type(conn: &Connection) -> DbResult<()> {
    let sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='document_import'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let Some(ddl) = sql else {
        return Ok(());
    };
    if ddl.contains("customer_quote") {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;
        CREATE TABLE document_import_v17 (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            doc_type TEXT NOT NULL
                CHECK (doc_type IN (
                    'supplier_invoice','customer_invoice','customer_quote',
                    'expense_receipt','bank_statement','ticket','unknown'
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
            invoice_id INTEGER,
            quote_id INTEGER,
            created_at TEXT NOT NULL,
            confirmed_at TEXT,
            source_fingerprint TEXT,
            source_origin_path TEXT
        );
        INSERT INTO document_import_v17(
            id, doc_type, source_file_name, stored_path, mime_type,
            extraction_method, extraction_json, confidence, status,
            entry_id, invoice_id, quote_id, created_at, confirmed_at,
            source_fingerprint, source_origin_path
        )
        SELECT id, doc_type, source_file_name, stored_path, mime_type,
            extraction_method, extraction_json, confidence, status,
            entry_id, invoice_id, quote_id, created_at, confirmed_at,
            source_fingerprint, source_origin_path
        FROM document_import;
        DROP TABLE document_import;
        ALTER TABLE document_import_v17 RENAME TO document_import;
        CREATE INDEX IF NOT EXISTS idx_document_import_status
            ON document_import(status);
        PRAGMA foreign_keys = ON;
        "#,
    )?;
    Ok(())
}

/// v16 — crée les fiches Factures manquantes pour les `customer_invoice` déjà confirmés
/// (imports antérieurs à #52 : VTE présente, `invoice_id` NULL). Idempotent.
///
/// Retourne le nombre de fiches nouvellement créées (les simples re-liens ne comptent pas).
pub fn backfill_imported_customer_invoices(conn: &Connection) -> DbResult<usize> {
    // Table absente (tests unitaires partiels) → no-op.
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='document_import'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(0);
    }
    // Cible principale : customer_invoice confirmés sans fiche Factures.
    // Élargi (best-effort) : confirmedDocType client, ou écriture journal VTE
    // (couvre un doc_type stocké incohérent après une revue mal classée).
    let mut stmt = conn.prepare(
        "SELECT di.id FROM document_import di
         LEFT JOIN entry e ON e.id = di.entry_id
         LEFT JOIN journal j ON j.id = e.journal_id
         WHERE di.status = 'confirmed'
           AND di.entry_id IS NOT NULL
           AND (di.invoice_id IS NULL OR di.invoice_id = 0)
           AND (
             di.doc_type = 'customer_invoice'
             OR json_extract(di.extraction_json, '$.confirmedDocType') = 'customer_invoice'
             OR j.code = 'VTE'
           )
         ORDER BY di.id",
    )?;
    let ids: Vec<i64> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);

    let mut created = 0usize;
    for id in ids {
        // Best-effort : une ligne legacy incohérente ne doit pas bloquer l’ouverture.
        match backfill_one_imported_customer_invoice(conn, id) {
            Ok(true) => created += 1,
            Ok(false) => {}
            Err(e) => {
                eprintln!("backfill Factures import #{id} skippé : {e}");
                // Skip — l’import reste sans invoice_id ; re-tentative au prochain open.
            }
        }
    }
    Ok(created)
}

fn json_i64(v: &serde_json::Value, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| {
        x.as_i64()
            .or_else(|| x.as_u64().map(|u| u as i64))
            .or_else(|| x.as_f64().map(|f| f as i64))
            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    })
}

fn json_str(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| {
        x.as_str()
            .map(|s| s.to_string())
            .or_else(|| x.as_i64().map(|n| n.to_string()))
    })
}

/// Relie ou crée une facture pour un import client confirmé. `true` = fiche créée.
fn backfill_one_imported_customer_invoice(conn: &Connection, import_id: i64) -> DbResult<bool> {
    let doc = get_document_import(conn, import_id)?;
    if doc.status != "confirmed" {
        return Ok(false);
    }
    if doc.invoice_id.is_some() {
        return Ok(false);
    }
    let entry_id = match doc.entry_id {
        Some(e) => e,
        None => return Ok(false),
    };

    let confirmed_as_customer = doc.doc_type == "customer_invoice"
        || json_str(&doc.extraction, "confirmedDocType")
            .as_deref()
            == Some("customer_invoice");

    // Déjà une fiche liée par document_import_id ou par entry_id → re-lier seulement.
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM invoice
             WHERE document_import_id = ?1
                OR (entry_id = ?2 AND source = 'import')
             ORDER BY id LIMIT 1",
            params![import_id, entry_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(inv_id) = existing {
        link_import_to_invoice(conn, &doc, inv_id)?;
        return Ok(false);
    }

    let entry = ledger::get_entry(conn, entry_id)?;
    let is_vte = entry.journal_code == "VTE";
    let has_411 = entry
        .lines
        .iter()
        .any(|l| l.account_number.starts_with("411") && l.debit_cents > 0);
    // ACH / frais : pas de fiche Factures. VTE + 411 (même si doc_type stocké faux) → oui.
    if !confirmed_as_customer && !(is_vte && has_411) {
        return Ok(false);
    }

    let tp_id = entry
        .lines
        .iter()
        .find_map(|l| {
            if l.account_number.starts_with("411") {
                l.third_party_id
            } else {
                None
            }
        })
        .or_else(|| {
            entry
                .lines
                .iter()
                .find_map(|l| if l.debit_cents > 0 { l.third_party_id } else { None })
        })
        .or_else(|| {
            entry
                .lines
                .iter()
                .find_map(|l| l.third_party_id)
        })
        .or_else(|| json_i64(&doc.extraction, "matchedThirdPartyId"))
        .ok_or_else(|| {
            format!(
                "Backfill import #{import_id} : client introuvable sur l’écriture VTE"
            )
        })?;

    let ht_from_ext = json_i64(&doc.extraction, "confirmedHtCents")
        .or_else(|| json_i64(&doc.extraction, "htCents"));
    let vat_from_ext = json_i64(&doc.extraction, "confirmedVatCents")
        .or_else(|| json_i64(&doc.extraction, "vatCents"));

    let ht_from_entry: i64 = entry
        .lines
        .iter()
        .filter(|l| l.account_number.starts_with('7') && l.credit_cents > 0)
        .map(|l| l.credit_cents)
        .sum();
    let vat_from_entry: i64 = entry
        .lines
        .iter()
        .filter(|l| l.account_number.starts_with("4457") && l.credit_cents > 0)
        .map(|l| l.credit_cents)
        .sum();
    let ar_debit: i64 = entry
        .lines
        .iter()
        .filter(|l| l.account_number.starts_with("411") && l.debit_cents > 0)
        .map(|l| l.debit_cents)
        .sum();
    let ht_from_ar = if ar_debit > 0 {
        (ar_debit - vat_from_entry.max(0)).max(0)
    } else {
        0
    };

    let ht = ht_from_ext
        .filter(|h| *h > 0)
        .unwrap_or_else(|| if ht_from_entry > 0 { ht_from_entry } else { ht_from_ar });
    if ht <= 0 {
        return Err(format!("Backfill import #{import_id} : montant HT invalide").into());
    }
    let vat_mode_raw = json_str(&doc.extraction, "confirmedVatMode")
        .or_else(|| json_str(&doc.extraction, "vatMode"))
        .unwrap_or_else(|| "normale".into());
    // Factures n’accepte que les régimes vente ; un régime ACH legacy → normale.
    let vat_mode = match vat_mode_raw.as_str() {
        "normale" | "sans" | "autoliquidation_chantier" => vat_mode_raw,
        _ => "normale".into(),
    };
    let rate_bp = json_i64(&doc.extraction, "rateBp").unwrap_or(2000) as i32;
    let vat = if vat_mode == "normale" {
        vat_from_ext
            .filter(|v| *v >= 0)
            .unwrap_or_else(|| {
                if vat_from_entry > 0 {
                    vat_from_entry
                } else {
                    compute_vat_cents(ht, rate_bp)
                }
            })
    } else {
        0
    };
    let vat_code = json_str(&doc.extraction, "vatCode").unwrap_or_else(|| "T20C".into());
    let ht_account = entry
        .lines
        .iter()
        .find(|l| l.credit_cents > 0 && l.account_number.starts_with('7'))
        .map(|l| l.account_number.clone())
        .or_else(|| json_str(&doc.extraction, "htAccount"))
        .unwrap_or_else(|| "706".into());
    let pdf_path = if doc.stored_path.trim().is_empty() {
        None
    } else {
        Some(doc.stored_path.as_str())
    };

    let tx = conn.unchecked_transaction()?;
    let inv = billing::create_imported_customer_invoice(
        &tx,
        import_id,
        tp_id,
        entry_id,
        &entry.entry_date,
        &entry.piece_ref,
        &entry.label,
        ht,
        vat,
        rate_bp,
        &vat_mode,
        &vat_code,
        &ht_account,
        pdf_path,
    )?;
    link_import_to_invoice(&tx, &doc, inv.id)?;
    tx.commit()?;
    Ok(true)
}

fn link_import_to_invoice(
    conn: &Connection,
    doc: &DocumentImport,
    invoice_id: i64,
) -> DbResult<()> {
    let mut extraction = doc.extraction.clone();
    if let Some(obj) = extraction.as_object_mut() {
        obj.insert("confirmedInvoiceId".into(), json!(invoice_id));
    }
    conn.execute(
        "UPDATE document_import SET invoice_id = ?1, extraction_json = ?2 WHERE id = ?3",
        params![invoice_id, extraction.to_string(), doc.id],
    )?;
    // Garantit le lien inverse si la fiche existait déjà sans document_import_id.
    let _ = conn.execute(
        "UPDATE invoice SET document_import_id = ?1, source = 'import'
         WHERE id = ?2 AND (document_import_id IS NULL OR document_import_id = 0)",
        params![doc.id, invoice_id],
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

/// Normalise le texte PDF (ligatures Adobe, espaces insécables) avant classification.
pub fn normalize_extracted_text(text: &str) -> String {
    let mut s = text.to_string();
    for (from, to) in [
        ("\u{FB00}", "ff"), // ﬀ
        ("\u{FB01}", "fi"), // ﬁ
        ("\u{FB02}", "fl"), // ﬂ
        ("\u{FB03}", "ffi"),
        ("\u{FB04}", "ffl"),
        ("\u{00A0}", " "),
        ("\u{202F}", " "),
        ("\u{2019}", "'"),
        ("\u{2018}", "'"),
        ("\u{201C}", "\""),
        ("\u{201D}", "\""),
    ] {
        if s.contains(from) {
            s = s.replace(from, to);
        }
    }
    s
}

/// Extraction locale : couche texte PDF d’abord. OCR cloud non requis.
pub fn extract_text(path: &Path) -> (String, String) {
    if is_pdf(path) {
        match pdf_extract::extract_text(path) {
            Ok(text) if text.chars().filter(|c| !c.is_whitespace()).count() >= 40 => {
                return (normalize_extracted_text(&text), "pdf_text".into());
            }
            Ok(text) if !text.trim().is_empty() => {
                // Texte trop pauvre → probablement scanné ; pas d’OCR cloud par défaut.
                return (
                    normalize_extracted_text(&text),
                    "image_pending_ocr".into(),
                );
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
        .filter(|c| {
            c.is_ascii_digit()
                || *c == ','
                || *c == '.'
                || *c == ' '
                || *c == '\u{00a0}'
                || *c == '\u{202f}'
        })
        .collect();
    let cleaned = cleaned
        .replace(' ', "")
        .replace('\u{00a0}', "")
        .replace('\u{202f}', "");
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

/// Extrait les montants FR d’une fenêtre en conservant les espaces milliers (`1 500,00`).
fn scan_fr_amounts(window: &str) -> Vec<i64> {
    let chars: Vec<char> = window.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut out = Vec::new();
    while i < n {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut raw = String::new();
        while i < n {
            let c = chars[i];
            if c.is_ascii_digit() {
                raw.push(c);
                i += 1;
            } else if (c == ' ' || c == '\u{00a0}' || c == '\u{202f}')
                && i + 1 < n
                && chars[i + 1].is_ascii_digit()
            {
                // séparateur de milliers — ne pas couper le nombre
                i += 1;
            } else if c == '.'
                && i + 1 < n
                && chars[i + 1].is_ascii_digit()
                && !raw.contains('.')
                && !raw.contains(',')
            {
                // millier (1.500,00) ou décimal (1500.00) — on garde et on normalise après
                raw.push('.');
                i += 1;
            } else if c == ',' && i + 1 < n && chars[i + 1].is_ascii_digit() {
                raw.push(',');
                i += 1;
                // jusqu’à 2 décimales
                let mut dec = 0;
                while i < n && chars[i].is_ascii_digit() && dec < 2 {
                    raw.push(chars[i]);
                    i += 1;
                    dec += 1;
                }
                break;
            } else {
                break;
            }
        }
        // Éviter les faux positifs : taux seuls « 0 » / « 20 » sans décimale si suivis de %
        let followed_by_percent = chars
            .get(i..)
            .map(|rest| {
                let s: String = rest.iter().take(4).collect::<String>();
                let t = s.trim_start();
                t.starts_with('%') || t.starts_with(" %")
            })
            .unwrap_or(false);
        if followed_by_percent && !raw.contains(',') && !raw.contains('.') {
            // laisser i où il est ; ne pas enregistrer le taux comme montant
            continue;
        }
        if let Some(v) = parse_fr_amount_to_cents(&raw) {
            // Ignorer les numéros trop longs (SIRET / IBAN fragments)
            if raw.chars().filter(|c| c.is_ascii_digit()).count() <= 10 {
                let _ = start;
                out.push(v);
            }
        }
    }
    out
}

fn label_match_indices(lower: &str, label: &str) -> Vec<usize> {
    let needle = label.to_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(&needle) {
        let idx = from + rel;
        out.push(idx);
        from = idx + needle.len().max(1);
        if from >= lower.len() {
            break;
        }
    }
    out
}

/// Cherche un montant après des libellés. `allow_zero` pour la TVA (0,00 €).
/// Préfère la **dernière** occurrence utile (évite l’en-tête de colonne « Total HT »).
fn find_amount_after(text: &str, labels: &[&str], allow_zero: bool) -> Option<i64> {
    let lower = text.to_lowercase();
    let mut best: Option<(usize, i64)> = None; // (label_index, amount)
    for label in labels {
        let needle = label.to_lowercase();
        for idx in label_match_indices(&lower, &needle) {
            let after = text.get(idx + label.len()..).unwrap_or("");
            // Fenêtre : ligne courante puis quelques lignes (montants souvent sous le libellé)
            let window: String = after.chars().take(96).collect();
            // En-tête de tableau : « Total HT » suivi de qty / taux → ignorer
            let window_l = window.to_lowercase();
            if window_l.contains("qté")
                || window_l.contains("qty")
                || window_l.contains("prix unitaire")
                || (window_l.contains("tva (%)") && !needle.contains("tva"))
            {
                continue;
            }
            let amounts = scan_fr_amounts(&window);
            let chosen = if allow_zero {
                amounts.into_iter().find(|v| *v >= 0)
            } else {
                amounts.into_iter().find(|v| *v > 0)
            };
            if let Some(v) = chosen {
                if best.map(|(bi, _)| idx >= bi).unwrap_or(true) {
                    best = Some((idx, v));
                }
            }
        }
    }
    best.map(|(_, v)| v)
}

fn find_vat_amount(text: &str) -> Option<i64> {
    // Libellés de montant TVA d’abord (y compris 0,00)
    if let Some(v) = find_amount_after(
        text,
        &[
            "montant total de la tva",
            "montant de la tva",
            "total tva",
            "montant tva",
            "dont tva",
        ],
        true,
    ) {
        return Some(v);
    }
    // « TVA 20 %   20,00 » — même ligne uniquement, en sautant le taux
    let lower = text.to_lowercase();
    for label in ["tva 20", "tva 10", "tva 5,5", "tva 5.5", "t.v.a.", "tva :", "tva:"] {
        for idx in label_match_indices(&lower, label) {
            let after = text.get(idx + label.len()..).unwrap_or("");
            let same_line: String = after.chars().take_while(|c| *c != '\n' && *c != '\r').collect();
            let amounts = scan_fr_amounts(&same_line);
            // Premier montant monétaire de la ligne (scan saute déjà les « 20 % »)
            if let Some(v) = amounts.into_iter().find(|v| *v > 0) {
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
            "base ht",
            "hors taxe",
            "hors taxes",
            "total h.t.",
            "total h.t",
            "ht :",
            "ht:",
        ],
        false,
    );
    let mut vat = find_vat_amount(text);
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
        false,
    );

    // Si TVA a capturé le TTC (ancien bug « TVA 20 % » → ligne Total TTC)
    if let (Some(v), Some(t)) = (vat, ttc) {
        if v == t {
            if let Some(h) = ht {
                if t >= h {
                    vat = Some(t - h);
                }
            } else {
                vat = None;
            }
        }
    }

    // Infer missing sides
    if ht.is_none() && ttc.is_some() && vat.is_some() {
        let t = ttc.unwrap();
        let v = vat.unwrap();
        if t >= v {
            ht = Some(t - v);
        }
    }
    if ttc.is_none() && ht.is_some() && vat.is_some() {
        ttc = Some(ht.unwrap() + vat.unwrap());
    }
    if vat.is_none() && ht.is_some() && ttc.is_some() && ttc.unwrap() >= ht.unwrap() {
        vat = Some(ttc.unwrap() - ht.unwrap());
    }
    // Cohérence finale HT + TVA = TTC
    if let (Some(h), Some(v), Some(t)) = (ht, vat, ttc) {
        if (h + v - t).abs() > 1 && t >= h {
            vat = Some(t - h);
        }
    }

    let rate_bp = match (ht, vat) {
        (Some(h), Some(v)) if h > 0 && v > 0 => {
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
        (Some(h), Some(0)) if h > 0 => {
            // TVA nulle affichée — pas de taux implicite 20 %
            Some(0)
        }
        _ => {
            let lower = text.to_lowercase();
            if lower.contains("0 %") || lower.contains("0%") {
                Some(0)
            } else if lower.contains("20 %") || lower.contains("20%") || lower.contains("tva 20") {
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
        "numéro de devis",
        "numero de devis",
        "n° de devis",
        "no de devis",
        "n° devis",
        "devis n°",
        "devis no",
        "devis:",
        "proposition n°",
        "proposition no",
        "numéro de facture",
        "numero de facture",
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

fn is_franchise_vat_mention(lower: &str) -> bool {
    lower.contains("franchise en base")
        || lower.contains("293 b")
        || lower.contains("293b")
        || lower.contains("article 293")
        || lower.contains("art. 293")
}

/// Mentions explicites d’autoliquidation / reverse charge (art. 283).
pub fn detect_reverse_charge(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("autoliquidation")
        || lower.contains("auto-liquidation")
        || lower.contains("auto liquidation")
        || lower.contains("art. 283")
        || lower.contains("art 283")
        || lower.contains("article 283")
        || lower.contains("tva due par le preneur")
        || lower.contains("due par le preneur")
        || lower.contains("autoliquide")
        || lower.contains("autoliquidée")
        || lower.contains("autoliquidee")
        || lower.contains("reverse charge")
        || lower.contains("vat reverse")
}

/// Autoliquidation implicite : TVA 0 affichée, HT = TTC, pas de franchise 293 B.
pub fn detect_reverse_charge_implicit(text: &str, amounts: &ParsedAmounts) -> bool {
    if detect_reverse_charge(text) {
        return true;
    }
    let lower = text.to_lowercase();
    if is_franchise_vat_mention(&lower) {
        return false;
    }
    let zero_rate_shown = lower.contains("0 %")
        || lower.contains("0%")
        || (lower.contains("tva (%)")
            && (lower.contains(" 0 %") || lower.contains("\t0 %") || lower.contains("\n0 %")));
    let zero_vat = amounts.vat_cents == Some(0) || zero_rate_shown;
    let ht_eq_ttc = matches!(
        (amounts.ht_cents, amounts.ttc_cents),
        (Some(h), Some(t)) if h == t && h > 0
    );
    // Facture (pas ticket) avec TVA à 0 et HT = TTC → reverse B2B typique
    zero_vat
        && ht_eq_ttc
        && lower.contains("facture")
        && (lower.contains("tva") || lower.contains("total ht"))
}

/// Contexte société (Ma SAS) pour distinguer émetteur (VTE) vs destinataire (ACH).
#[derive(Debug, Clone, Copy)]
pub struct OrgClassifyHint<'a> {
    pub name: &'a str,
    pub siret: Option<&'a str>,
}

fn normalize_digits(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Positions (byte index) des suites de 9–14 chiffres (SIREN/SIRET), espaces ignorés.
fn siret_like_positions(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].1.is_ascii_digit() {
            let start = chars[i].0;
            let mut digits = String::new();
            let mut j = i;
            while j < chars.len() {
                let c = chars[j].1;
                if c.is_ascii_digit() {
                    digits.push(c);
                    j += 1;
                } else if (c == ' ' || c == '\u{00a0}')
                    && j + 1 < chars.len()
                    && chars[j + 1].1.is_ascii_digit()
                    && digits.len() < 14
                {
                    j += 1;
                } else {
                    break;
                }
                if digits.len() >= 14 {
                    break;
                }
            }
            if digits.len() == 14 || digits.len() == 9 {
                out.push((start, digits));
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

fn org_appears_as_issuer(text: &str, org: &OrgClassifyHint<'_>) -> bool {
    let lower = text.to_lowercase();
    let name = org.name.trim().to_lowercase();
    let mut name_hit = false;
    if name.len() >= 3 {
        if let Some(pos) = lower.find(&name) {
            // Nom société dans la première moitié → souvent en-tête émetteur
            name_hit = pos < lower.len() / 2;
        }
    }
    if let Some(siret) = org.siret {
        let dig = normalize_digits(siret);
        if dig.len() >= 9 {
            let runs = siret_like_positions(text);
            if let Some((pos, _)) = runs.iter().find(|(_, d)| d.starts_with(&dig) || dig.starts_with(d))
            {
                // Org = premier SIRET/SIREN du document → émetteur
                if runs.first().map(|(p, _)| p) == Some(pos) {
                    return true;
                }
                // Sinon, SIRET org encore dans la première moitié du texte
                if *pos < text.len() / 2 {
                    return true;
                }
            }
        }
    }
    name_hit
}

fn org_appears_as_buyer(text: &str, org: &OrgClassifyHint<'_>) -> bool {
    let lower = text.to_lowercase();
    let name = org.name.trim().to_lowercase();
    for marker in [
        "facturé à",
        "facture a",
        "client :",
        "client:",
        "destinataire",
        "à l'attention",
        "a l'attention",
        "livré à",
        "livre a",
    ] {
        if let Some(idx) = lower.find(marker) {
            let window: String = lower.chars().skip(idx).take(160).collect();
            if name.len() >= 3 && window.contains(&name) {
                return true;
            }
            if let Some(siret) = org.siret {
                let dig = normalize_digits(siret);
                if dig.len() >= 9 && normalize_digits(&window).contains(&dig) {
                    return true;
                }
            }
        }
    }
    false
}

/// Classifiers typés — score indépendant ; le meilleur gagne si confiance ≥ seuil.
pub fn classify_document(text: &str) -> (String, f64) {
    classify_document_with_org(text, None)
}

pub fn classify_document_with_org(
    text: &str,
    org: Option<OrgClassifyHint<'_>>,
) -> (String, f64) {
    let lower = text.to_lowercase();
    let mut scores: Vec<(&str, f64)> = DOC_TYPES
        .iter()
        .copied()
        .map(|t| (t, 0.0))
        .collect();

    // « subrogaon » / « aﬀacturage » : artefacts pdf-extract (lettre manquante / ligature).
    // « devis associé » / « numéro du devis » = facture qui référence un devis → vente, pas devis.
    let invoice_refs_quote = lower.contains("devis associé")
        || lower.contains("devis associe")
        || lower.contains("numéro du devis")
        || lower.contains("numero du devis");
    let outbound_sale_signals = invoice_refs_quote
        || lower.contains("affacturage")
        || lower.contains("affactur")
        || lower.contains("subrogation")
        || lower.contains("subroga")
        || lower.contains("bon de commande")
        || lower.contains("conditions de règlement")
        || lower.contains("conditions de reglement")
        || lower.contains("merci de votre confiance");

    let quote_doc_signals = (lower.contains("devis")
        || lower.contains("proposition commerciale")
        || lower.contains("offre de prix")
        || lower.contains("offre commerciale"))
        && !invoice_refs_quote
        && !lower.contains("facture");
    let quote_soft = !invoice_refs_quote
        && (lower.contains("devis n")
            || lower.contains("devis:")
            || lower.contains("validité de l'offre")
            || lower.contains("validite de l'offre")
            || lower.contains("valable jusqu")
            || lower.contains("proposition commerciale"));

    let buyer_block = lower.contains("facturé à")
        || lower.contains("facture a")
        || lower.contains("client :")
        || lower.contains("client:")
        || lower.contains("à votre attention")
        || lower.contains("a votre attention");

    for (t, s) in scores.iter_mut() {
        *s = match *t {
            "supplier_invoice" => {
                let mut v: f64 = 0.0;
                if lower.contains("facture") {
                    v += 0.25;
                }
                // Signaux acheteur (nous recevons) — « net à payer » seulement hors vente sortante
                if lower.contains("fournisseur") {
                    v += 0.25;
                }
                if lower.contains("facturé à") || lower.contains("facture a") {
                    // « Facturé à : … » → facture reçue (ACH) sauf si org = émetteur
                    v += 0.2;
                }
                if lower.contains("total ht") || lower.contains("total ttc") {
                    v += 0.15;
                }
                if !outbound_sale_signals
                    && (lower.contains("net à payer") || lower.contains("net a payer"))
                {
                    v += 0.15;
                }
                if detect_reverse_charge(&lower) {
                    v += 0.15;
                }
                if lower.contains("sous-traitance") {
                    v += 0.1;
                }
                if outbound_sale_signals {
                    v -= 0.4;
                }
                if lower.contains("à votre attention")
                    || lower.contains("merci de votre confiance")
                    || (lower.contains("client :") || lower.contains("client:"))
                {
                    v -= 0.3;
                }
                if let Some(o) = org {
                    if org_appears_as_issuer(text, &o) && !org_appears_as_buyer(text, &o) {
                        v -= 0.45;
                    }
                    if org_appears_as_buyer(text, &o) {
                        v += 0.4;
                    }
                }
                v.max(0.0).min(0.98)
            }
            "customer_invoice" => {
                let mut v: f64 = 0.0;
                if lower.contains("facture") {
                    v += 0.2;
                }
                if buyer_block {
                    v += 0.35;
                }
                if outbound_sale_signals {
                    v += 0.35;
                }
                if lower.contains("total ht") || lower.contains("total ttc") {
                    v += 0.1;
                }
                // Ne plus pénaliser « à payer » / « net à payer » (standard aussi en vente)
                if lower.contains("fournisseur") && !buyer_block {
                    v -= 0.25;
                }
                if quote_doc_signals {
                    v -= 0.45;
                }
                if let Some(o) = org {
                    if org_appears_as_issuer(text, &o) && !org_appears_as_buyer(text, &o) {
                        v += 0.5;
                    }
                    if org_appears_as_buyer(text, &o) {
                        v -= 0.4;
                    }
                }
                v.max(0.0).min(0.98)
            }
            "customer_quote" => {
                let mut v: f64 = 0.0;
                if quote_doc_signals {
                    v += 0.55;
                }
                if quote_soft {
                    v += 0.35;
                }
                if lower.contains("total ht") || lower.contains("total ttc") {
                    v += 0.1;
                }
                if lower.contains("validité")
                    || lower.contains("validite")
                    || lower.contains("valable jusqu")
                {
                    v += 0.15;
                }
                if invoice_refs_quote || lower.contains("facture") {
                    v -= 0.5;
                }
                if let Some(o) = org {
                    if org_appears_as_issuer(text, &o) && !org_appears_as_buyer(text, &o) {
                        v += 0.25;
                    }
                    if org_appears_as_buyer(text, &o) {
                        v -= 0.35;
                    }
                }
                v.max(0.0).min(0.95)
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
        "customer_invoice" | "customer_quote" => Some("customer"),
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
    let reverse = detect_reverse_charge_implicit(text, &amounts)
        && matches!(
            doc_type,
            "supplier_invoice" | "customer_invoice" | "customer_quote"
        );
    let is_sale = matches!(doc_type, "customer_invoice" | "customer_quote");
    let is_quote = doc_type == "customer_quote";
    let is_bank = doc_type == "bank_statement";
    let vat_mode = if reverse && doc_type == "supplier_invoice" {
        "autoliquidation_sous_traitance"
    } else if reverse && is_sale {
        "autoliquidation_chantier"
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
    // Autoliquidation : la facture affiche souvent 0 % — conserver 20 % pour le calcul ACH (44566/4452).
    let rate = match amounts.rate_bp {
        Some(0) if reverse => 2000,
        Some(r) => r,
        None => 2000,
    };
    let vat_code = match (vat_mode, rate, is_sale) {
        ("autoliquidation_sous_traitance", 1000, _) => "T10R",
        ("autoliquidation_sous_traitance", 550, _) => "T55R",
        ("autoliquidation_sous_traitance", _, _) => "T20R",
        ("autoliquidation_chantier", _, _) => "T20C",
        ("sans", _, _) => "TEX",
        (_, 1000, true) => "T10C",
        (_, 550, true) => "T55C",
        (_, _, true) => "T20C",
        (_, 1000, false) => "T10D",
        (_, 550, false) => "T55D",
        _ => "T20D",
    };
    let ht_account = default_ht_account_for(doc_type, reverse && doc_type == "supplier_invoice");
    let counterpart = default_counterpart_for(doc_type);
    let journal = default_journal_for(doc_type);
    let bank_direction = if is_bank { Some("debit") } else { None };
    let notes = if method == "image_pending_ocr" {
        "Texte peu lisible ou image scannée : OCR local non activé (pas d’API cloud). Vérifiez / saisissez les montants."
    } else if is_quote {
        "Devis détecté : à la confirmation, une fiche Commercial → Devis sera créée (numéro externe conservé). Aucune écriture comptable."
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
        invoice_id: r.get(10)?,
        quote_id: r.get(11)?,
        created_at: r.get(12)?,
        confirmed_at: r.get(13)?,
        source_fingerprint: r.get(14)?,
        source_origin_path: r.get(15)?,
    })
}

const SELECT_COLS: &str = "id, doc_type, source_file_name, stored_path, mime_type,
    extraction_method, extraction_json, confidence, status, entry_id, invoice_id,
    quote_id, created_at, confirmed_at, source_fingerprint, source_origin_path";

/// Empreinte SHA-256 hex du contenu (idempotence import).
pub fn file_fingerprint(path: &Path) -> DbResult<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn find_active_by_fingerprint(
    conn: &Connection,
    fingerprint: &str,
) -> DbResult<Option<DocumentImport>> {
    conn.query_row(
        &format!(
            "SELECT {SELECT_COLS} FROM document_import
             WHERE source_fingerprint = ?1 AND status != 'discarded'
             ORDER BY id DESC LIMIT 1"
        ),
        params![fingerprint],
        row_to_import,
    )
    .optional()
    .map_err(Into::into)
}

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
        // File active seulement : ne pas remonter les imports soft-discarded historiques.
        let mut stmt = conn.prepare(&format!(
            "SELECT {SELECT_COLS} FROM document_import
             WHERE status IN ('pending_review', 'confirmed')
             ORDER BY id DESC"
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
    analyze_document_with_hint(conn, workspace_path, source_path, None)
}

/// Comme [`analyze_document`], avec indice de type optionnel (compagnon mobile).
pub fn analyze_document_with_hint(
    conn: &Connection,
    workspace_path: &Path,
    source_path: &Path,
    type_hint: Option<&str>,
) -> DbResult<DocumentImport> {
    analyze_document_idempotent(conn, workspace_path, source_path, type_hint, true)
}

/// Classifie + construit le payload d’extraction (partagé insert / refresh).
fn analyze_text_pipeline(
    conn: &Connection,
    text: &str,
    method: &str,
    type_hint: Option<&str>,
    fingerprint: &str,
    origin: &str,
) -> DbResult<(String, f64, String, serde_json::Value)> {
    let org = super::load_organization(conn).ok();
    let org_hint = org.as_ref().map(|o| OrgClassifyHint {
        name: o.name.as_str(),
        siret: o.siret.as_deref(),
    });
    let (mut doc_type, mut confidence) = classify_document_with_org(text, org_hint);
    // Facture ambiguë : sans signaux vente / org émetteur → privilégier fournisseur.
    if doc_type == "unknown" {
        let amounts = parse_amounts(text);
        let lower = text.to_lowercase();
        let sale_like = lower.contains("à votre attention")
            || lower.contains("conditions de règlement")
            || lower.contains("devis associé")
            || lower.contains("affacturage")
            || lower.contains("affactur")
            || lower.contains("subroga")
            || org_hint
                .map(|o| org_appears_as_issuer(text, &o) && !org_appears_as_buyer(text, &o))
                .unwrap_or(false);
        if (amounts.ttc_cents.is_some() || amounts.ht_cents.is_some())
            && lower.contains("facture")
            && !sale_like
        {
            doc_type = "supplier_invoice".into();
            confidence = confidence.max(0.55);
        } else if sale_like
            && (amounts.ttc_cents.is_some() || amounts.ht_cents.is_some())
            && lower.contains("facture")
        {
            doc_type = "customer_invoice".into();
            confidence = confidence.max(0.55);
        }
    }
    if let Some(hint) = type_hint {
        if DOC_TYPES.contains(&hint) && hint != "unknown" {
            doc_type = hint.to_string();
            confidence = confidence.max(0.72);
        }
    }
    let siret = parse_siret(text);
    let party = parse_supplier_name(text);
    let kind = party_kind_for_doc(&doc_type).unwrap_or("supplier");
    let matched = match_third_party(conn, kind, party.as_deref(), siret.as_deref())?;
    let mut payload = build_extraction_payload(
        text,
        method,
        &doc_type,
        confidence,
        matched.as_ref(),
    );
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("sourceFingerprint".into(), json!(fingerprint));
        obj.insert("sourceOriginPath".into(), json!(origin));
    }
    Ok((doc_type, confidence, method.to_string(), payload))
}

/// Rafraîchit l’extraction d’un import encore en revue (applique correctifs OCR sans doublon).
fn refresh_pending_import(
    conn: &Connection,
    existing: &DocumentImport,
    source_path: &Path,
    type_hint: Option<&str>,
    fingerprint: &str,
    origin: &str,
) -> DbResult<DocumentImport> {
    let extract_path = if Path::new(&existing.stored_path).is_file() {
        Path::new(&existing.stored_path)
    } else {
        source_path
    };
    let (text, method) = extract_text(extract_path);
    let (doc_type, confidence, method, payload) =
        analyze_text_pipeline(conn, &text, &method, type_hint, fingerprint, origin)?;
    conn.execute(
        "UPDATE document_import SET
            doc_type = ?1,
            extraction_method = ?2,
            extraction_json = ?3,
            confidence = ?4,
            source_origin_path = ?5
         WHERE id = ?6 AND status = 'pending_review'",
        params![
            doc_type,
            method,
            payload.to_string(),
            confidence,
            origin,
            existing.id
        ],
    )?;
    get_document_import(conn, existing.id)
}

/// Analyse + copie ; si `idempotent` :
/// - confirmé → renvoie l’existant (pas de doublon)
/// - en revue → **re-extrait** sur la même ligne (correctifs OCR / Ma SAS s’appliquent)
pub fn analyze_document_idempotent(
    conn: &Connection,
    workspace_path: &Path,
    source_path: &Path,
    type_hint: Option<&str>,
    idempotent: bool,
) -> DbResult<DocumentImport> {
    if !source_path.is_file() {
        return Err("Fichier source introuvable".into());
    }
    let fingerprint = file_fingerprint(source_path)?;
    let origin = source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf())
        .display()
        .to_string();
    if idempotent {
        if let Some(existing) = find_active_by_fingerprint(conn, &fingerprint)? {
            if existing.status == "confirmed" {
                return Ok(existing);
            }
            if existing.status == "pending_review" {
                return refresh_pending_import(
                    conn,
                    &existing,
                    source_path,
                    type_hint,
                    &fingerprint,
                    &origin,
                );
            }
            return Ok(existing);
        }
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
    let (doc_type, confidence, method, payload) =
        analyze_text_pipeline(conn, &text, &method, type_hint, &fingerprint, &origin)?;
    let mime = guess_mime(source_path);
    let now = now_iso();
    conn.execute(
        "INSERT INTO document_import(
            doc_type, source_file_name, stored_path, mime_type,
            extraction_method, extraction_json, confidence, status,
            entry_id, created_at, confirmed_at, source_fingerprint, source_origin_path
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending_review', NULL, ?8, NULL, ?9, ?10)",
        params![
            doc_type,
            file_name,
            dest.display().to_string(),
            mime,
            method,
            payload.to_string(),
            confidence,
            now,
            fingerprint,
            origin
        ],
    )?;
    let id = conn.last_insert_rowid();
    get_document_import(conn, id)
}

/// Supprime définitivement un import en revue (fichier + ligne) — pas de passage en « Traité ».
pub fn discard_document_import(conn: &Connection, id: i64) -> DbResult<()> {
    let doc = get_document_import(conn, id)?;
    if doc.status == "confirmed" {
        return Err("Import déjà confirmé — ne peut pas être supprimé".into());
    }
    if !doc.stored_path.is_empty() {
        let _ = fs::remove_file(&doc.stored_path);
    }
    conn.execute("DELETE FROM document_import WHERE id = ?1", params![id])?;
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

/// Confirmation d’un devis importé → fiche Commercial / Devis (pas d’écriture).
fn confirm_customer_quote_import(
    conn: &Connection,
    id: i64,
    doc: &DocumentImport,
    input: &ConfirmDocumentImportInput,
) -> DbResult<DocumentImport> {
    let ht = input.ht_cents;
    if ht <= 0 {
        return Err("Montant HT invalide pour le devis".into());
    }
    let vat_mode = input.vat_mode.as_str();
    if !matches!(
        vat_mode,
        "normale" | "sans" | "autoliquidation_chantier"
    ) {
        return Err("Régime TVA invalide pour un devis client".into());
    }
    let rate = input.rate_bp.max(0);
    let vat_code = if input.vat_code.trim().is_empty() {
        "T20C".to_string()
    } else {
        input.vat_code.trim().to_string()
    };
    tax::ensure_vat_code(conn, &vat_code)?;
    let ht_account = if input.ht_account.trim().is_empty() {
        "706".to_string()
    } else {
        input.ht_account.trim().to_string()
    };
    let (tp_id, _) = ensure_party_for_confirm(conn, input, "customer", true)?
        .ok_or_else(|| "Client requis".to_string())?;
    let quote_vat = if vat_mode == "normale" {
        input
            .vat_cents
            .filter(|v| *v >= 0)
            .unwrap_or_else(|| compute_vat_cents(ht, rate))
    } else {
        0
    };
    let label = if input.label.trim().is_empty() {
        let piece = input.piece_ref.trim().to_string().if_empty("devis");
        format!("Import {piece}")
    } else {
        input.label.trim().to_string()
    };

    let tx = conn.unchecked_transaction()?;
    let quote = billing::create_imported_quote(
        &tx,
        id,
        tp_id,
        &input.entry_date,
        &input.piece_ref,
        &label,
        ht,
        quote_vat,
        rate,
        vat_mode,
        &vat_code,
        &ht_account,
        Some(doc.stored_path.as_str()),
    )?;

    let mut extraction = doc.extraction.clone();
    if let Some(obj) = extraction.as_object_mut() {
        obj.insert("confirmedDocType".into(), json!("customer_quote"));
        obj.insert("confirmedHtCents".into(), json!(ht));
        obj.insert("confirmedVatMode".into(), json!(vat_mode));
        obj.insert("confirmedQuoteId".into(), json!(quote.id));
        obj.insert("confirmedQuoteNumber".into(), json!(quote.number.clone()));
    }
    let now = now_iso();
    let updated = tx.execute(
        "UPDATE document_import SET
            doc_type = 'customer_quote',
            extraction_json = ?1,
            status = 'confirmed',
            quote_id = ?2,
            confirmed_at = ?3
         WHERE id = ?4 AND status = 'pending_review' AND quote_id IS NULL AND entry_id IS NULL",
        params![extraction.to_string(), quote.id, now, id],
    )?;
    if updated != 1 {
        return Err(
            "Confirmation concurrente ou état incohérent — aucun devis en double n'a été créé"
                .into(),
        );
    }
    tx.commit()?;
    get_document_import(conn, id)
}

/// Confirmation humaine obligatoire → écriture **postée** + pièce jointe liée
/// (ou fiche devis commerciale pour `customer_quote`, sans écriture).
///
/// Flux comptable : `create_draft` → pièce jointe (exige encore draft) → `post_entry`.
/// Flux devis : `create_imported_quote` uniquement (Commercial → Devis).
/// La revue Import reste le seul filet humain ; pas de 2ᵉ validation Écritures.
pub fn confirm_document_import(
    conn: &Connection,
    id: i64,
    input: &ConfirmDocumentImportInput,
) -> DbResult<DocumentImport> {
    let doc = get_document_import(conn, id)?;
    if doc.status != "pending_review" {
        return Err("Seuls les imports en revue peuvent être confirmés".into());
    }
    // Défense anti double-post : un entry_id orphelin (crash avant UPDATE) ne doit
    // jamais permettre une 2ᵉ confirmation → 2ᵉ écriture postée.
    if doc.entry_id.is_some() {
        return Err(
            "Import déjà lié à une écriture — confirmation impossible (évite un double post)"
                .into(),
        );
    }
    if doc.quote_id.is_some() {
        return Err(
            "Import déjà lié à un devis — confirmation impossible (évite un doublon)"
                .into(),
        );
    }
    let doc_type = if input.doc_type.trim().is_empty() {
        doc.doc_type.clone()
    } else {
        input.doc_type.trim().to_string()
    };
    if !DOC_TYPES.contains(&doc_type.as_str()) {
        return Err("Type de document invalide".into());
    }
    if !confirm_supported(&doc_type) {
        return Err(
            "Précisez le type de document (inconnu ne crée pas d’écriture ni de devis)."
                .into(),
        );
    }
    if doc_type == "customer_quote" {
        return confirm_customer_quote_import(conn, id, &doc, input);
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
        "customer_invoice" | "customer_quote" => "T20C",
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

    let mut customer_party_id: Option<i64> = None;
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
            customer_party_id = Some(tp_id);
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

    // Transaction atomique : post + passage confirmed. Sinon un crash après
    // `post_entry` laisse l'import en `pending_review` → double confirmation = double post.
    let tx = conn.unchecked_transaction()?;
    let draft = ledger::create_draft(
        &tx,
        &CreateEntryInput {
            journal_code: journal.clone(),
            entry_date: input.entry_date.clone(),
            piece_ref: input.piece_ref.clone(),
            label,
            lines,
        },
    )?;
    let mime = doc.mime_type.clone();
    if let Err(e) = ledger::add_attachment(
        &tx,
        draft.id,
        &doc.source_file_name,
        &doc.stored_path,
        mime,
    ) {
        let _ = ledger::delete_draft(&tx, draft.id);
        return Err(e);
    }
    // Attach while draft, then post so Grand livre / TVA / FEC see the entry.
    let entry = match ledger::post_entry(&tx, draft.id) {
        Ok(e) => e,
        Err(e) => {
            let _ = ledger::delete_draft(&tx, draft.id);
            return Err(e);
        }
    };

    // Facture client importée → fiche Factures émise (Encaisser) liée à la VTE.
    let mut invoice_id: Option<i64> = None;
    if doc_type == "customer_invoice" {
        let tp_id = customer_party_id.ok_or_else(|| "Client requis".to_string())?;
        let invoice_vat = if vat_mode == "normale" {
            input
                .vat_cents
                .filter(|v| *v >= 0)
                .unwrap_or_else(|| compute_vat_cents(ht, rate))
        } else {
            0
        };
        match billing::create_imported_customer_invoice(
            &tx,
            id,
            tp_id,
            entry.id,
            &input.entry_date,
            &input.piece_ref,
            &input.label,
            ht,
            invoice_vat,
            rate,
            vat_mode,
            &vat_code,
            &ht_account,
            Some(doc.stored_path.as_str()),
        ) {
            Ok(inv) => invoice_id = Some(inv.id),
            Err(e) => {
                // Rollback TX (pas de commit) — pas d’écriture orpheline ni de fiche Factures.
                return Err(e);
            }
        }
    }

    let mut extraction = doc.extraction;
    if let Some(obj) = extraction.as_object_mut() {
        obj.insert("confirmedDocType".into(), json!(doc_type));
        obj.insert("confirmedHtCents".into(), json!(ht));
        obj.insert("confirmedVatMode".into(), json!(vat_mode));
        obj.insert("confirmedJournal".into(), json!(journal));
        obj.insert("confirmedEntryId".into(), json!(entry.id));
        if let Some(ref num) = entry.ecriture_num {
            obj.insert("confirmedEcritureNum".into(), json!(num));
        }
        if let Some(inv_id) = invoice_id {
            obj.insert("confirmedInvoiceId".into(), json!(inv_id));
        }
    }
    let now = now_iso();
    let updated = tx.execute(
        "UPDATE document_import SET
            doc_type = ?1,
            extraction_json = ?2,
            status = 'confirmed',
            entry_id = ?3,
            invoice_id = ?4,
            confirmed_at = ?5
         WHERE id = ?6 AND status = 'pending_review' AND entry_id IS NULL",
        params![
            doc_type,
            extraction.to_string(),
            entry.id,
            invoice_id,
            now,
            id
        ],
    )?;
    if updated != 1 {
        return Err(
            "Confirmation concurrente ou état incohérent — aucun double post n'a été créé"
                .into(),
        );
    }
    tx.commit()?;
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
    fn parse_amounts_skips_tva_rate_line_grabbing_ttc() {
        // Régression : « TVA 20 % » seul puis Total TTC ne doit pas mettre 120 en TVA.
        let text = r#"
Total HT                  100,00
TVA 20 %
Total TTC                 120,00
"#;
        let a = parse_amounts(text);
        assert_eq!(a.ht_cents, Some(10_000));
        assert_eq!(a.ttc_cents, Some(12_000));
        assert_eq!(a.vat_cents, Some(2_000));
    }

    #[test]
    fn parse_amounts_thousand_spaces_and_summary_not_column_header() {
        let text = include_str!("fixtures/import_ocr/2611-F-2026-1132-3PSI.txt");
        let a = parse_amounts(text);
        assert_eq!(a.ht_cents, Some(150_000), "HT 1 500,00 — got {a:?}");
        assert_eq!(a.vat_cents, Some(0), "TVA 0,00 — got {a:?}");
        assert_eq!(a.ttc_cents, Some(150_000), "TTC 1 500,00 — got {a:?}");
        assert_eq!(a.rate_bp, Some(0));
    }

    #[test]
    fn parse_and_classify_real_pdf_extract_dump() {
        // Dump pdf-extract (ligatures déjà normalisées dans le fichier).
        let text = include_str!("fixtures/import_ocr/2611-F-2026-1132-3PSI.pdf-extract.txt");
        let a = parse_amounts(text);
        assert_eq!(a.ht_cents, Some(150_000), "{a:?}");
        assert_eq!(a.vat_cents, Some(0), "{a:?}");
        assert_eq!(a.ttc_cents, Some(150_000), "{a:?}");
        let (t, c) = classify_document(text);
        assert_eq!(t, "customer_invoice", "confidence {c}");
        assert!(detect_reverse_charge_implicit(text, &a));
    }

    #[test]
    fn classify_fixture_3psi_client_invoice_as_vte() {
        let text = include_str!("fixtures/import_ocr/2611-F-2026-1132-3PSI.txt");
        let org = OrgClassifyHint {
            name: "3PSI",
            siret: Some("93362796000018"),
        };
        let (t, c) = classify_document_with_org(text, Some(org));
        assert_eq!(t, "customer_invoice", "confidence {c}");
        assert!(c >= 0.45, "confidence {c}");
        // Même sans org : affacturage + devis associé → vente
        let (t2, c2) = classify_document(text);
        assert_eq!(t2, "customer_invoice", "sans org confidence {c2}");
    }

    #[test]
    fn classify_supplier_when_org_is_buyer() {
        let text = include_str!("fixtures/import_ocr/supplier-normale.txt");
        let org = OrgClassifyHint {
            name: "3PSI",
            siret: Some("93362796000018"),
        };
        let (t, c) = classify_document_with_org(text, Some(org));
        assert_eq!(t, "supplier_invoice", "confidence {c}");
    }

    #[test]
    fn fixture_3psi_autoliquidation_chantier_on_zero_vat() {
        let text = include_str!("fixtures/import_ocr/2611-F-2026-1132-3PSI.txt");
        let amounts = parse_amounts(text);
        assert!(detect_reverse_charge_implicit(text, &amounts));
        let payload = build_extraction_payload(text, "pdf_text", "customer_invoice", 0.8, None);
        assert_eq!(
            payload.get("vatMode").and_then(|v| v.as_str()),
            Some("autoliquidation_chantier")
        );
        assert_eq!(payload.get("htCents").and_then(|v| v.as_i64()), Some(150_000));
        assert_eq!(payload.get("vatCents").and_then(|v| v.as_i64()), Some(0));
        assert_eq!(payload.get("ttcCents").and_then(|v| v.as_i64()), Some(150_000));
        assert_eq!(payload.get("journalCode").and_then(|v| v.as_str()), Some("VTE"));
        assert_eq!(payload.get("reverseCharge").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn fixture_supplier_autoliquidation_283() {
        let text = include_str!("fixtures/import_ocr/supplier-autoliquidation-283.txt");
        assert!(detect_reverse_charge(text));
        let amounts = parse_amounts(text);
        assert_eq!(amounts.ht_cents, Some(200_000));
        assert_eq!(amounts.vat_cents, Some(0));
        assert_eq!(amounts.ttc_cents, Some(200_000));
        let (t, _) = classify_document_with_org(
            text,
            Some(OrgClassifyHint {
                name: "3PSI",
                siret: Some("93362796000018"),
            }),
        );
        assert_eq!(t, "supplier_invoice");
        let payload = build_extraction_payload(text, "pdf_text", "supplier_invoice", 0.8, None);
        assert_eq!(
            payload.get("vatMode").and_then(|v| v.as_str()),
            Some("autoliquidation_sous_traitance")
        );
        assert_eq!(payload.get("htAccount").and_then(|v| v.as_str()), Some("604"));
    }

    #[test]
    fn analyze_real_pdf_fixture_3psi_f_2026_1132() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();
        // Aligner Ma SAS sur l’émetteur du PDF (3PSI)
        conn.execute(
            "UPDATE organization SET name = '3PSI', siret = '93362796000018', siren = '933627960'",
            [],
        )
        .unwrap();

        let pdf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/db/fixtures/import_ocr/2611-F-2026-1132-3PSI.pdf");
        assert!(pdf.is_file(), "missing fixture PDF {pdf:?}");
        let doc = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        assert_eq!(doc.doc_type, "customer_invoice");
        let ht = doc.extraction.get("htCents").and_then(|v| v.as_i64());
        let vat = doc.extraction.get("vatCents").and_then(|v| v.as_i64());
        let ttc = doc.extraction.get("ttcCents").and_then(|v| v.as_i64());
        let mode = doc.extraction.get("vatMode").and_then(|v| v.as_str());
        let journal = doc.extraction.get("journalCode").and_then(|v| v.as_str());
        // pdf-extract may scramble layout; text fixture covers exact amounts.
        // PDF path must at least classify VTE and not put TTC into TVA as a huge error.
        assert_eq!(journal, Some("VTE"));
        assert!(
            matches!(mode, Some("autoliquidation_chantier") | Some("sans")),
            "vatMode={mode:?}"
        );
        if let (Some(h), Some(v), Some(t)) = (ht, vat, ttc) {
            assert_eq!(h, 150_000, "ht={h} vat={v} ttc={t} extraction={:?}", doc.extraction);
            assert_eq!(v, 0);
            assert_eq!(t, 150_000);
            assert_eq!(mode, Some("autoliquidation_chantier"));
        }
        assert_ne!(vat, Some(150_000), "TVA must not receive the TTC amount");
        assert_ne!(ht, Some(100), "must not parse '1' from '1 500,00' as HT");
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
    fn analyze_with_hint_overrides_classifier() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) =
            db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("ticket-hint.pdf");
        write_simple_pdf(&pdf, "Juste un montant 12,50 EUR");
        let doc = analyze_document_with_hint(
            &conn,
            Path::new(&info.path),
            &pdf,
            Some("ticket"),
        )
        .unwrap();
        assert_eq!(doc.doc_type, "ticket");
        assert!(doc.confidence >= 0.72);
        assert_eq!(doc.status, "pending_review");
        assert!(doc.source_fingerprint.is_some());
    }

    #[test]
    fn analyze_same_bytes_is_idempotent() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) =
            db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("same.pdf");
        write_simple_pdf(&pdf, "FACTURE FA-IDEMP\nTotal TTC 10,00");
        let a = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        let copy = dir.path().join("copy-same.pdf");
        fs::copy(&pdf, &copy).unwrap();
        let b = analyze_document(&conn, Path::new(&info.path), &copy).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.source_fingerprint, b.source_fingerprint);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM document_import", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn normalize_ligatures_unlocks_affacturage_signal() {
        let raw = "Facture\na\u{FB00}acturage Bibby\nsubrogaon dans le cadre";
        let norm = normalize_extracted_text(raw);
        assert!(norm.to_lowercase().contains("affacturage"));
        let (t, _) = classify_document(&norm);
        // Pas assez de signaux seuls — on vérifie surtout la normalisation.
        assert!(norm.contains("affacturage"));
        let _ = t;
    }

    #[test]
    fn reanalyze_pending_same_fingerprint_refreshes_bad_extraction() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) =
            db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();
        conn.execute(
            "UPDATE organization SET name = '3PSI', siret = '93362796000018', siren = '933627960'",
            [],
        )
        .unwrap();

        let pdf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/db/fixtures/import_ocr/2611-F-2026-1132-3PSI.pdf");
        assert!(pdf.is_file());
        let fp = file_fingerprint(&pdf).unwrap();
        let dest = imports_dir(Path::new(&info.path)).join("stale.pdf");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(&pdf, &dest).unwrap();
        // Ancien import erroné (pré-0.6.8) encore en revue — même empreinte.
        conn.execute(
            "INSERT INTO document_import(
                doc_type, source_file_name, stored_path, mime_type,
                extraction_method, extraction_json, confidence, status,
                entry_id, created_at, confirmed_at, source_fingerprint, source_origin_path
             ) VALUES (
                'supplier_invoice', '2611-F-2026-1132-3PSI.pdf', ?1, 'application/pdf',
                'pdf_text', ?2, 0.9, 'pending_review',
                NULL, '2026-01-01T00:00:00Z', NULL, ?3, ?4
             )",
            params![
                dest.display().to_string(),
                r#"{"htCents":100,"vatCents":150000,"ttcCents":150000,"vatMode":"normale","journalCode":"ACH","docType":"supplier_invoice"}"#,
                fp,
                pdf.display().to_string()
            ],
        )
        .unwrap();
        let stale_id: i64 = conn.last_insert_rowid();

        let refreshed = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        assert_eq!(refreshed.id, stale_id, "doit recycler la ligne en revue");
        assert_eq!(refreshed.doc_type, "customer_invoice");
        let vat = refreshed.extraction.get("vatCents").and_then(|v| v.as_i64());
        let journal = refreshed
            .extraction
            .get("journalCode")
            .and_then(|v| v.as_str());
        assert_ne!(vat, Some(150_000), "TVA ne doit plus porter le TTC");
        assert_eq!(journal, Some("VTE"));
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM document_import", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn discard_hard_deletes_row_not_soft_traite() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) =
            db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("to-delete.pdf");
        write_simple_pdf(&pdf, "FACTURE FA-DEL\nTotal TTC 10,00");
        let doc = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        let stored = doc.stored_path.clone();
        assert!(Path::new(&stored).is_file());
        discard_document_import(&conn, doc.id).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM document_import WHERE id = ?1", [doc.id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 0);
        assert!(!Path::new(&stored).is_file());
        let listed = list_document_imports(&conn, None).unwrap();
        assert!(listed.iter().all(|d| d.status != "discarded"));
    }

    #[test]
    fn analyze_confirm_posts_entry_with_attachment() {
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
        assert_eq!(entry.status, "posted");
        assert!(entry.ecriture_num.as_deref().unwrap_or("").starts_with("ACH-"));
        assert_eq!(entry.journal_code, "ACH");
        assert_eq!(entry.attachments.len(), 1);
        let d: i64 = entry.lines.iter().map(|l| l.debit_cents).sum();
        let c: i64 = entry.lines.iter().map(|l| l.credit_cents).sum();
        assert_eq!(d, c);
        assert_eq!(d, 12_000);

        let filter = crate::models::LedgerFilter {
            from_date: Some("2026-01-01".into()),
            to_date: Some("2026-12-31".into()),
            journal_code: Some("ACH".into()),
            account_number: None,
        };
        let moves = crate::db::reporting::ledger_movements(&conn, &filter).unwrap();
        assert!(
            moves.iter().any(|m| m.entry_id == entry_id),
            "posted import must appear in Grand livre movements"
        );
    }

    #[test]
    fn confirm_customer_invoice_creates_vte_posted() {
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
        assert_eq!(entry.status, "posted");
        assert_eq!(entry.journal_code, "VTE");
        assert!(entry.ecriture_num.as_deref().unwrap_or("").starts_with("VTE-"));
        assert!(entry.lines.iter().any(|l| l.account_number == "411"));
        assert_eq!(entry.attachments.len(), 1);

        let inv_id = confirmed.invoice_id.expect("invoice Factures liée");
        let inv = billing::get_invoice(&conn, inv_id).unwrap();
        assert_eq!(inv.source, "import");
        assert_eq!(inv.status, "issued");
        assert_eq!(inv.number.as_deref(), Some("FAC-1"));
        assert_eq!(inv.entry_id, confirmed.entry_id);
        assert_eq!(inv.document_import_id, Some(confirmed.id));
        assert_eq!(inv.ht_cents, 10_000);
        assert_eq!(inv.vat_cents, 2_000);
        assert_eq!(inv.ttc_cents, 12_000);
        assert_eq!(inv.paid_cents, 0);

        let pay = billing::record_payment(
            &conn,
            &crate::models::CreatePaymentInput {
                invoice_id: inv.id,
                paid_on: "2026-03-20".into(),
                amount_cents: 5_000,
                method: "virement".into(),
                bank_ref: Some("VIR-IMP".into()),
            },
        )
        .unwrap();
        let bq_id = pay.entry_id.expect("BQ");
        let bq = ledger::get_entry(&conn, bq_id).unwrap();
        assert_eq!(bq.status, "posted");
        assert_eq!(bq.journal_code, "BQ");
        let partial = billing::get_invoice(&conn, inv.id).unwrap();
        assert_eq!(partial.status, "partially_paid");
        assert_eq!(partial.paid_cents, 5_000);
        let expected_lettre = "FAC1";
        assert_eq!(pay.lettre.as_deref(), Some(expected_lettre));
        let vte_lettre: String = conn
            .query_row(
                "SELECT ecriture_lettre FROM entry WHERE id = ?1",
                params![entry.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(vte_lettre, expected_lettre);
    }

    #[test]
    fn classify_standalone_devis_as_customer_quote() {
        let text = "DEVIS N° D-2024-0012\nProposition commerciale\nClient ACME SAS\n\
            Total HT 1 200,00\nTVA 20 % 240,00\nTotal TTC 1 440,00\nValable jusqu'au 30/06/2024";
        let (t, c) = classify_document(text);
        assert_eq!(t, "customer_quote", "confidence {c}");
        assert!(c >= 0.45, "confidence {c}");
        assert_eq!(parse_invoice_number(text).as_deref(), Some("D-2024-0012"));
    }

    #[test]
    fn classify_invoice_referencing_devis_stays_customer_invoice() {
        let text = include_str!("fixtures/import_ocr/2611-F-2026-1132-3PSI.txt");
        let (t, _) = classify_document(text);
        assert_eq!(t, "customer_invoice");
    }

    #[test]
    fn confirm_customer_quote_creates_commercial_devis_without_entry() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("ancien-devis.pdf");
        write_simple_pdf(
            &pdf,
            "DEVIS N° D-LEG-42\nProposition commerciale\nClient ACME\n\
             Total HT 100,00\nTVA 20,00\nTotal TTC 120,00\nValable jusqu'au 31/12/2026",
        );
        let doc = analyze_document_with_hint(
            &conn,
            Path::new(&info.path),
            &pdf,
            Some("customer_quote"),
        )
        .unwrap();
        assert_eq!(doc.doc_type, "customer_quote");

        let confirmed = confirm_document_import(
            &conn,
            doc.id,
            &ConfirmDocumentImportInput {
                doc_type: "customer_quote".into(),
                entry_date: "2024-06-01".into(),
                piece_ref: "D-LEG-42".into(),
                label: "Import D-LEG-42".into(),
                journal_code: "DEV".into(),
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
                create_supplier_code: Some("C910".into()),
                create_supplier_siret: None,
            },
        )
        .unwrap();
        assert_eq!(confirmed.status, "confirmed");
        assert!(confirmed.entry_id.is_none());
        assert!(confirmed.invoice_id.is_none());
        let qid = confirmed.quote_id.expect("devis Commercial lié");
        let q = billing::get_quote(&conn, qid).unwrap();
        assert_eq!(q.source, "import");
        assert_eq!(q.status, "sent");
        assert_eq!(q.number, "D-LEG-42");
        assert_eq!(q.document_import_id, Some(confirmed.id));
        assert_eq!(q.ht_cents, 10_000);
        assert_eq!(q.vat_cents, 2_000);
        assert_eq!(q.ttc_cents, 12_000);
        assert!(!q.lines.is_empty());

        // Re-confirmation refusée ; même empreinte renvoie l’existant.
        let err = confirm_document_import(
            &conn,
            confirmed.id,
            &ConfirmDocumentImportInput {
                doc_type: "customer_quote".into(),
                entry_date: "2024-06-01".into(),
                piece_ref: "D-LEG-42".into(),
                label: "Import D-LEG-42".into(),
                journal_code: "DEV".into(),
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
                create_supplier_code: Some("C910".into()),
                create_supplier_siret: None,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("revue") || err.to_string().contains("déjà"));

        let again = analyze_document_idempotent(
            &conn,
            Path::new(&info.path),
            &pdf,
            Some("customer_quote"),
            true,
        )
        .unwrap();
        assert_eq!(again.id, confirmed.id);
        assert_eq!(again.quote_id, Some(qid));
        let imported: Vec<_> = billing::list_quotes(&conn)
            .unwrap()
            .into_iter()
            .filter(|q| q.source == "import" && q.document_import_id == Some(confirmed.id))
            .collect();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].id, qid);
    }

    #[test]
    fn confirm_supplier_invoice_does_not_create_billing_invoice() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("fa-fourn.pdf");
        write_simple_pdf(
            &pdf,
            "FACTURE FOURNISSEUR\nFA-99\nTotal HT 50,00\nTVA 10,00\nTotal TTC 60,00",
        );
        let doc = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        let before = billing::list_invoices(&conn).unwrap().len();
        let confirmed = confirm_document_import(
            &conn,
            doc.id,
            &ConfirmDocumentImportInput {
                doc_type: "supplier_invoice".into(),
                entry_date: "2026-03-15".into(),
                piece_ref: "FA-99".into(),
                label: "Import FA-99".into(),
                journal_code: "ACH".into(),
                ht_cents: 5_000,
                vat_cents: Some(1_000),
                rate_bp: 2000,
                vat_mode: "normale".into(),
                vat_code: "T20D".into(),
                ht_account: "606".into(),
                counterpart_account: None,
                bank_direction: None,
                third_party_id: None,
                create_supplier_name: Some("FOURN X".into()),
                create_supplier_code: Some("F901".into()),
                create_supplier_siret: None,
            },
        )
        .unwrap();
        assert!(confirmed.invoice_id.is_none());
        assert_eq!(billing::list_invoices(&conn).unwrap().len(), before);
    }

    #[test]
    fn confirm_ticket_creates_od_posted() {
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
        assert_eq!(entry.status, "posted");
        assert_eq!(entry.journal_code, "OD");
        assert!(entry.ecriture_num.as_deref().unwrap_or("").starts_with("OD-"));
        assert!(entry.lines.iter().any(|l| l.account_number == "530"));
    }

    #[test]
    fn confirm_rejects_orphan_entry_id_to_prevent_double_post() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("fa-orphan.pdf");
        write_simple_pdf(&pdf, sample_invoice_text());
        let doc = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        let input = ConfirmDocumentImportInput {
            doc_type: "supplier_invoice".into(),
            entry_date: "2026-03-15".into(),
            piece_ref: "FA-ORPHAN".into(),
            label: "Import orphan guard".into(),
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
            create_supplier_code: Some("F901".into()),
            create_supplier_siret: Some("12345678900012".into()),
        };
        let confirmed = confirm_document_import(&conn, doc.id, &input).unwrap();
        let first_entry = confirmed.entry_id.expect("entry");

        // Simule un crash historique : écriture postée mais statut encore en revue.
        conn.execute(
            "UPDATE document_import SET status = 'pending_review', confirmed_at = NULL WHERE id = ?1",
            params![doc.id],
        )
        .unwrap();
        let posted_before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entry WHERE status = 'posted' AND journal_id = (SELECT id FROM journal WHERE code = 'ACH')",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let err = confirm_document_import(&conn, doc.id, &input).unwrap_err();
        assert!(
            err.to_string().contains("déjà lié") || err.to_string().contains("double post"),
            "unexpected err: {err}"
        );
        let posted_after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entry WHERE status = 'posted' AND journal_id = (SELECT id FROM journal WHERE code = 'ACH')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(posted_before, posted_after, "aucun second post ACH");
        assert_eq!(
            ledger::get_entry(&conn, first_entry).unwrap().status,
            "posted"
        );

        // Double confirm normal (déjà confirmed) refusé aussi.
        conn.execute(
            "UPDATE document_import SET status = 'confirmed', confirmed_at = ?1 WHERE id = ?2",
            params![now_iso(), doc.id],
        )
        .unwrap();
        assert!(confirm_document_import(&conn, doc.id, &input).is_err());
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

    /// Simule un import client confirmé avant #52 : VTE présente, pas de fiche Factures.
    fn strip_invoice_like_pre_pr52(conn: &Connection, import_id: i64, invoice_id: i64) {
        conn.execute(
            "DELETE FROM invoice_line WHERE invoice_id = ?1",
            params![invoice_id],
        )
        .unwrap();
        conn.execute("DELETE FROM invoice WHERE id = ?1", params![invoice_id])
            .unwrap();
        let doc = get_document_import(conn, import_id).unwrap();
        let mut extraction = doc.extraction;
        if let Some(obj) = extraction.as_object_mut() {
            obj.remove("confirmedInvoiceId");
        }
        conn.execute(
            "UPDATE document_import SET invoice_id = NULL, extraction_json = ?1 WHERE id = ?2",
            params![extraction.to_string(), import_id],
        )
        .unwrap();
        let after = get_document_import(conn, import_id).unwrap();
        assert!(after.entry_id.is_some());
        assert!(after.invoice_id.is_none());
    }

    #[test]
    fn backfill_creates_invoice_for_legacy_confirmed_customer_import() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("fa-legacy.pdf");
        write_simple_pdf(
            &pdf,
            "FACTURE N° LEG-42\nÀ votre attention\nClient ACME\nTotal HT 100,00\nTVA 20,00\nTotal TTC 120,00",
        );
        let doc = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        let confirmed = confirm_document_import(
            &conn,
            doc.id,
            &ConfirmDocumentImportInput {
                doc_type: "customer_invoice".into(),
                entry_date: "2026-03-15".into(),
                piece_ref: "LEG-42".into(),
                label: "Import LEG-42".into(),
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
                create_supplier_name: Some("ACME LEGACY".into()),
                create_supplier_code: Some("C942".into()),
                create_supplier_siret: None,
            },
        )
        .unwrap();
        let inv_id = confirmed.invoice_id.expect("créée par #52");
        let entry_id = confirmed.entry_id.expect("VTE");
        strip_invoice_like_pre_pr52(&conn, confirmed.id, inv_id);

        let before = billing::list_invoices(&conn).unwrap().len();
        let created = backfill_imported_customer_invoices(&conn).unwrap();
        assert_eq!(created, 1);
        let after = billing::list_invoices(&conn).unwrap();
        assert_eq!(after.len(), before + 1);

        let linked = get_document_import(&conn, confirmed.id).unwrap();
        let new_inv_id = linked.invoice_id.expect("backfill invoice_id");
        let inv = billing::get_invoice(&conn, new_inv_id).unwrap();
        assert_eq!(inv.source, "import");
        assert_eq!(inv.status, "issued");
        assert_eq!(inv.number.as_deref(), Some("LEG-42"));
        assert_eq!(inv.entry_id, Some(entry_id));
        assert_eq!(inv.document_import_id, Some(confirmed.id));
        assert_eq!(inv.ht_cents, 10_000);
        assert_eq!(inv.vat_cents, 2_000);
        assert_eq!(inv.ttc_cents, 12_000);
        assert_eq!(
            linked
                .extraction
                .get("confirmedInvoiceId")
                .and_then(|v| v.as_i64()),
            Some(new_inv_id)
        );

        // Idempotent : 2ᵉ passage ne crée pas de doublon.
        let created2 = backfill_imported_customer_invoices(&conn).unwrap();
        assert_eq!(created2, 0);
        assert_eq!(billing::list_invoices(&conn).unwrap().len(), after.len());
        let again = get_document_import(&conn, confirmed.id).unwrap();
        assert_eq!(again.invoice_id, Some(new_inv_id));
    }

    #[test]
    fn backfill_via_migrate_on_workspace_open() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("fa-mig.pdf");
        write_simple_pdf(
            &pdf,
            "FACTURE N° MIG-1\nÀ votre attention\nClient X\nTotal HT 50,00\nTVA 10,00\nTotal TTC 60,00",
        );
        let doc = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        let confirmed = confirm_document_import(
            &conn,
            doc.id,
            &ConfirmDocumentImportInput {
                doc_type: "customer_invoice".into(),
                entry_date: "2026-04-01".into(),
                piece_ref: "MIG-1".into(),
                label: "Import MIG-1".into(),
                journal_code: "VTE".into(),
                ht_cents: 5_000,
                vat_cents: Some(1_000),
                rate_bp: 2000,
                vat_mode: "normale".into(),
                vat_code: "T20C".into(),
                ht_account: "706".into(),
                counterpart_account: Some("411".into()),
                bank_direction: None,
                third_party_id: None,
                create_supplier_name: Some("CLIENT MIG".into()),
                create_supplier_code: Some("C943".into()),
                create_supplier_siret: None,
            },
        )
        .unwrap();
        strip_invoice_like_pre_pr52(&conn, confirmed.id, confirmed.invoice_id.unwrap());
        conn.execute(
            "UPDATE meta SET value = '15' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();

        // Réouverture / migrate (comme open_workspace) doit remonter la fiche.
        db::migrate(&conn).unwrap();
        let version: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, db::SCHEMA_VERSION.to_string());
        let linked = get_document_import(&conn, confirmed.id).unwrap();
        let inv_id = linked.invoice_id.expect("migrate backfill");
        let inv = billing::get_invoice(&conn, inv_id).unwrap();
        assert_eq!(inv.number.as_deref(), Some("MIG-1"));
        assert_eq!(inv.source, "import");
        assert_eq!(inv.entry_id, confirmed.entry_id);
    }

    #[test]
    fn backfill_skips_supplier_and_relinks_existing_invoice() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        // Fournisseur confirmé : backfill ne crée rien.
        let pdf_s = dir.path().join("fa-fourn-bf.pdf");
        write_simple_pdf(
            &pdf_s,
            "FACTURE FOURNISSEUR\nFA-BF\nTotal HT 80,00\nTVA 16,00\nTotal TTC 96,00",
        );
        let doc_s = analyze_document(&conn, Path::new(&info.path), &pdf_s).unwrap();
        let supplier = confirm_document_import(
            &conn,
            doc_s.id,
            &ConfirmDocumentImportInput {
                doc_type: "supplier_invoice".into(),
                entry_date: "2026-03-15".into(),
                piece_ref: "FA-BF".into(),
                label: "Import FA-BF".into(),
                journal_code: "ACH".into(),
                ht_cents: 8_000,
                vat_cents: Some(1_600),
                rate_bp: 2000,
                vat_mode: "normale".into(),
                vat_code: "T20D".into(),
                ht_account: "606".into(),
                counterpart_account: None,
                bank_direction: None,
                third_party_id: None,
                create_supplier_name: Some("FOURN BF".into()),
                create_supplier_code: Some("F942".into()),
                create_supplier_siret: None,
            },
        )
        .unwrap();
        assert!(supplier.invoice_id.is_none());
        let before = billing::list_invoices(&conn).unwrap().len();
        assert_eq!(backfill_imported_customer_invoices(&conn).unwrap(), 0);
        assert_eq!(billing::list_invoices(&conn).unwrap().len(), before);

        // Client : fiche existe déjà (document_import_id) mais invoice_id null → re-lien.
        let pdf_c = dir.path().join("fa-relink.pdf");
        write_simple_pdf(
            &pdf_c,
            "FACTURE N° REL-1\nÀ votre attention\nClient Y\nTotal HT 20,00\nTVA 4,00\nTotal TTC 24,00",
        );
        let doc_c = analyze_document(&conn, Path::new(&info.path), &pdf_c).unwrap();
        let customer = confirm_document_import(
            &conn,
            doc_c.id,
            &ConfirmDocumentImportInput {
                doc_type: "customer_invoice".into(),
                entry_date: "2026-05-01".into(),
                piece_ref: "REL-1".into(),
                label: "Import REL-1".into(),
                journal_code: "VTE".into(),
                ht_cents: 2_000,
                vat_cents: Some(400),
                rate_bp: 2000,
                vat_mode: "normale".into(),
                vat_code: "T20C".into(),
                ht_account: "706".into(),
                counterpart_account: Some("411".into()),
                bank_direction: None,
                third_party_id: None,
                create_supplier_name: Some("CLIENT REL".into()),
                create_supplier_code: Some("C944".into()),
                create_supplier_siret: None,
            },
        )
        .unwrap();
        let inv_id = customer.invoice_id.unwrap();
        // Orpheline côté import seulement (facture conserve document_import_id).
        conn.execute(
            "UPDATE document_import SET invoice_id = NULL WHERE id = ?1",
            params![customer.id],
        )
        .unwrap();
        let created = backfill_imported_customer_invoices(&conn).unwrap();
        assert_eq!(created, 0, "re-lien, pas de nouvelle fiche");
        let linked = get_document_import(&conn, customer.id).unwrap();
        assert_eq!(linked.invoice_id, Some(inv_id));
        assert_eq!(
            billing::list_invoices(&conn)
                .unwrap()
                .iter()
                .filter(|i| i.document_import_id == Some(customer.id))
                .count(),
            1
        );
    }

    #[test]
    fn backfill_recovers_vte_when_stored_doc_type_wrong() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let pdf = dir.path().join("fa-vte-wrong-type.pdf");
        write_simple_pdf(
            &pdf,
            "FACTURE N° WRONG-1\nÀ votre attention\nClient Z\nTotal HT 30,00\nTVA 6,00\nTotal TTC 36,00",
        );
        let doc = analyze_document(&conn, Path::new(&info.path), &pdf).unwrap();
        let confirmed = confirm_document_import(
            &conn,
            doc.id,
            &ConfirmDocumentImportInput {
                doc_type: "customer_invoice".into(),
                entry_date: "2026-06-01".into(),
                piece_ref: "WRONG-1".into(),
                label: "Import WRONG-1".into(),
                journal_code: "VTE".into(),
                ht_cents: 3_000,
                vat_cents: Some(600),
                rate_bp: 2000,
                vat_mode: "normale".into(),
                vat_code: "T20C".into(),
                ht_account: "706".into(),
                counterpart_account: Some("411".into()),
                bank_direction: None,
                third_party_id: None,
                create_supplier_name: Some("CLIENT WRONG".into()),
                create_supplier_code: Some("C945".into()),
                create_supplier_siret: None,
            },
        )
        .unwrap();
        let inv_id = confirmed.invoice_id.expect("créée par #52");
        strip_invoice_like_pre_pr52(&conn, confirmed.id, inv_id);
        // Simule un doc_type stocké incohérent (revue / legacy) tout en gardant VTE + 411.
        conn.execute(
            "UPDATE document_import SET doc_type = 'supplier_invoice' WHERE id = ?1",
            params![confirmed.id],
        )
        .unwrap();

        let created = backfill_imported_customer_invoices(&conn).unwrap();
        assert_eq!(created, 1);
        let linked = get_document_import(&conn, confirmed.id).unwrap();
        let new_inv = billing::get_invoice(&conn, linked.invoice_id.unwrap()).unwrap();
        assert_eq!(new_inv.source, "import");
        assert_eq!(new_inv.number.as_deref(), Some("WRONG-1"));
        assert_eq!(new_inv.entry_id, confirmed.entry_id);
    }
}
