//! Module commercial — devis, factures (schéma), PDF, hooks d'écriture VTE.

use super::ledger;
use super::DbResult;
use crate::models::*;
use chrono::{Duration, NaiveDate, Utc};
use printpdf::path::PaintMode;
use printpdf::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs::File;
use std::io::{BufWriter, Cursor};
use std::path::Path;

/// Logo 3PSI embarqué (PNG) — rendu PDF offline (Tauri desktop).
const COMMERCIAL_LOGO_PNG: &[u8] = include_bytes!("../../../src/assets/brand/3psi-logo.png");
/// Largeur/hauteur logo sur A4 — calé sur la référence 3PSI (~94,5 pt ≈ 33,34 mm).
const COMMERCIAL_LOGO_MM: f32 = 33.34;
/// Hauteur page A4 (mm) — origine PDF en bas à gauche.
const PAGE_H_MM: f32 = 297.0;

pub const SCHEMA_BILLING_VERSION: i32 = 12;

/// Séries documentaires configurables (compteur annuel calendaire).
/// Les factures (standard / acompte / solde / avoir) partagent la série FACT
/// (`F` + `AAAA-MMNN`) ; les devis utilisent la série DEV (`D` + `AAAA-MMNN`).
pub const DOCUMENT_SEQUENCE_KINDS: &[&str] = &["FACT", "DEV"];

const QUOTE_STATUSES: &[&str] = &[
    "draft",
    "sent",
    "accepted",
    "refused",
    "expired",
    "invoiced",
];

const INVOICE_KINDS: &[&str] = &["standard", "deposit", "balance", "credit_note"];
const INVOICE_STATUSES: &[&str] = &[
    "draft",
    "issued",
    "sent",
    "partially_paid",
    "paid",
    "cancelled",
];

pub fn migrate_billing(conn: &Connection) -> DbResult<()> {
    enrich_third_party(conn)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS document_sequence (
            kind TEXT NOT NULL,
            fiscal_year_id INTEGER NOT NULL REFERENCES fiscal_year(id),
            next_num INTEGER NOT NULL DEFAULT 1 CHECK (next_num >= 1),
            PRIMARY KEY (kind, fiscal_year_id)
        );

        -- Compteur annuel calendaire (AAAA), indépendant de l’exercice comptable.
        CREATE TABLE IF NOT EXISTS document_sequence_cal (
            kind TEXT NOT NULL,
            calendar_year TEXT NOT NULL,
            next_num INTEGER NOT NULL DEFAULT 1 CHECK (next_num >= 1),
            PRIMARY KEY (kind, calendar_year)
        );

        CREATE TABLE IF NOT EXISTS quote (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            number TEXT NOT NULL UNIQUE,
            third_party_id INTEGER NOT NULL REFERENCES third_party(id),
            status TEXT NOT NULL DEFAULT 'draft'
                CHECK (status IN ('draft','sent','accepted','refused','expired','invoiced')),
            issue_date TEXT NOT NULL,
            valid_until TEXT,
            currency TEXT NOT NULL DEFAULT 'EUR',
            ht_cents INTEGER NOT NULL DEFAULT 0,
            vat_cents INTEGER NOT NULL DEFAULT 0,
            ttc_cents INTEGER NOT NULL DEFAULT 0,
            vat_mode TEXT NOT NULL DEFAULT 'normale',
            notes TEXT,
            legal_mentions TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS quote_line (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            quote_id INTEGER NOT NULL REFERENCES quote(id) ON DELETE CASCADE,
            line_no INTEGER NOT NULL,
            label TEXT NOT NULL,
            description TEXT,
            quantity_milli INTEGER NOT NULL DEFAULT 1000 CHECK (quantity_milli > 0),
            unit TEXT NOT NULL DEFAULT 'u',
            unit_price_cents INTEGER NOT NULL DEFAULT 0,
            ht_cents INTEGER NOT NULL DEFAULT 0,
            vat_rate_bp INTEGER NOT NULL DEFAULT 0,
            vat_code TEXT,
            account_number TEXT NOT NULL DEFAULT '706'
        );

        CREATE TABLE IF NOT EXISTS invoice (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            number TEXT,
            kind TEXT NOT NULL DEFAULT 'standard'
                CHECK (kind IN ('standard','deposit','balance','credit_note')),
            third_party_id INTEGER NOT NULL REFERENCES third_party(id),
            quote_id INTEGER REFERENCES quote(id),
            parent_invoice_id INTEGER REFERENCES invoice(id),
            status TEXT NOT NULL DEFAULT 'draft'
                CHECK (status IN ('draft','issued','sent','partially_paid','paid','cancelled')),
            issue_date TEXT NOT NULL,
            due_date TEXT,
            service_date TEXT,
            ht_cents INTEGER NOT NULL DEFAULT 0,
            vat_cents INTEGER NOT NULL DEFAULT 0,
            ttc_cents INTEGER NOT NULL DEFAULT 0,
            paid_cents INTEGER NOT NULL DEFAULT 0,
            vat_mode TEXT NOT NULL DEFAULT 'normale',
            vat_code TEXT,
            einvoice_profile TEXT NOT NULL DEFAULT 'none',
            einvoice_status TEXT NOT NULL DEFAULT 'local_only',
            pdp_message_id TEXT,
            entry_id INTEGER REFERENCES entry(id),
            pdf_path TEXT,
            facturx_path TEXT,
            notes TEXT,
            legal_mentions TEXT,
            purchase_order_number TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'native'
                CHECK (source IN ('native','import')),
            document_import_id INTEGER
        );

        CREATE TABLE IF NOT EXISTS invoice_line (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            invoice_id INTEGER NOT NULL REFERENCES invoice(id) ON DELETE CASCADE,
            line_no INTEGER NOT NULL,
            label TEXT NOT NULL,
            description TEXT,
            quantity_milli INTEGER NOT NULL DEFAULT 1000 CHECK (quantity_milli > 0),
            unit TEXT NOT NULL DEFAULT 'u',
            unit_price_cents INTEGER NOT NULL DEFAULT 0,
            ht_cents INTEGER NOT NULL DEFAULT 0,
            vat_rate_bp INTEGER NOT NULL DEFAULT 0,
            vat_code TEXT,
            account_number TEXT NOT NULL DEFAULT '706',
            analytic_code TEXT
        );

        CREATE TABLE IF NOT EXISTS payment (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            invoice_id INTEGER NOT NULL REFERENCES invoice(id) ON DELETE CASCADE,
            paid_on TEXT NOT NULL,
            amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
            method TEXT NOT NULL DEFAULT 'virement',
            bank_ref TEXT,
            entry_id INTEGER REFERENCES entry(id)
        );

        CREATE TABLE IF NOT EXISTS reminder (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            invoice_id INTEGER NOT NULL REFERENCES invoice(id) ON DELETE CASCADE,
            level INTEGER NOT NULL CHECK (level IN (7, 15, 30)),
            sent_at TEXT,
            channel TEXT NOT NULL DEFAULT 'pdf',
            body_snapshot TEXT
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_invoice_number_unique
            ON invoice(number) WHERE number IS NOT NULL;
        "#,
    )?;
    enrich_billing_line_description(conn)?;
    enrich_invoice_import_link(conn)?;
    enrich_invoice_purchase_order(conn)?;
    Ok(())
}

/// v12 billing — numéro de bon de commande (saisie manuelle facture).
fn enrich_invoice_purchase_order(conn: &Connection) -> DbResult<()> {
    let cols = table_columns(conn, "invoice")?;
    if !cols.iter().any(|c| c == "purchase_order_number") {
        conn.execute(
            "ALTER TABLE invoice ADD COLUMN purchase_order_number TEXT",
            [],
        )?;
    }
    Ok(())
}

/// v15 — origine import + lien document_import (workspaces antérieurs).
fn enrich_invoice_import_link(conn: &Connection) -> DbResult<()> {
    let cols = table_columns(conn, "invoice")?;
    if !cols.iter().any(|c| c == "source") {
        conn.execute(
            "ALTER TABLE invoice ADD COLUMN source TEXT NOT NULL DEFAULT 'native'",
            [],
        )?;
    }
    if !cols.iter().any(|c| c == "document_import_id") {
        conn.execute(
            "ALTER TABLE invoice ADD COLUMN document_import_id INTEGER",
            [],
        )?;
    }
    Ok(())
}

fn enrich_third_party(conn: &Connection) -> DbResult<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(third_party)")?;
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();
    for (col, ddl) in [
        ("siret", "ALTER TABLE third_party ADD COLUMN siret TEXT"),
        ("email", "ALTER TABLE third_party ADD COLUMN email TEXT"),
        ("address", "ALTER TABLE third_party ADD COLUMN address TEXT"),
        (
            "routing_id",
            "ALTER TABLE third_party ADD COLUMN routing_id TEXT",
        ),
    ] {
        if !cols.iter().any(|c| c == col) {
            conn.execute(ddl, [])?;
        }
    }
    Ok(())
}

fn table_columns(conn: &Connection, table: &str) -> DbResult<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(cols)
}

fn enrich_billing_line_description(conn: &Connection) -> DbResult<()> {
    for (table, ddl) in [
        (
            "quote_line",
            "ALTER TABLE quote_line ADD COLUMN description TEXT",
        ),
        (
            "invoice_line",
            "ALTER TABLE invoice_line ADD COLUMN description TEXT",
        ),
    ] {
        let cols = table_columns(conn, table)?;
        if !cols.iter().any(|c| c == "description") {
            conn.execute(ddl, [])?;
        }
    }
    Ok(())
}

fn now_iso() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn open_fy_id(conn: &Connection) -> DbResult<i64> {
    conn.query_row(
        "SELECT id FROM fiscal_year WHERE status = 'open' ORDER BY id DESC LIMIT 1",
        [],
        |r| r.get(0),
    )
    .map_err(|_| "Aucun exercice ouvert".into())
}

/// Année civile AAAA pour les compteurs documentaires.
///
/// Priorité : `label` si exactement 4 chiffres ; sinon année de `start_date`.
/// Évite la divergence Ma SAS (compteurs sur label libre) vs allocation
/// (`issue_date` année calendaire).
fn calendar_year_for_open_fy(conn: &Connection, fy_id: i64) -> DbResult<String> {
    let (label, start): (String, String) = conn.query_row(
        "SELECT label, start_date FROM fiscal_year WHERE id = ?1",
        params![fy_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if label.len() == 4 && label.chars().all(|c| c.is_ascii_digit()) {
        return Ok(label);
    }
    if start.len() >= 4 && start.as_bytes()[0..4].iter().all(u8::is_ascii_digit) {
        return Ok(start[0..4].to_string());
    }
    Err(format!(
        "Exercice #{fy_id} : impossible de dériver une année civile (label={label}, start={start})"
    )
    .into())
}

fn default_due_date(issue_date: &str) -> Option<String> {
    NaiveDate::parse_from_str(issue_date, "%Y-%m-%d")
        .ok()
        .map(|d| (d + Duration::days(30)).format("%Y-%m-%d").to_string())
}

fn sequence_label_fr(kind: &str) -> String {
    match kind {
        "FACT" => "Factures (standard, acompte, solde, avoir)".into(),
        "DEV" => "Devis".into(),
        other => other.to_string(),
    }
}

fn validate_sequence_kind(kind: &str) -> DbResult<()> {
    if DOCUMENT_SEQUENCE_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(format!(
            "Série inconnue « {kind} » (attendu : {}). \
             Acomptes et avoirs partagent la série Factures.",
            DOCUMENT_SEQUENCE_KINDS.join(", ")
        )
        .into())
    }
}

/// Préfixe de série : `F` factures (y compris acomptes / avoirs), `D` devis.
fn document_number_letter(kind: &str) -> &'static str {
    if kind == "DEV" {
        "D"
    } else {
        "F"
    }
}

/// Format d’émission : `F2026-1101` / `D2026-1101`
/// (`lettre` + année + `-` + mois collé à la séquence annuelle sur 2 chiffres).
fn format_document_number(kind: &str, year: &str, month: &str, num: i64) -> String {
    let letter = document_number_letter(kind);
    format!("{letter}{year}-{month}{num:02}")
}

fn parse_issue_year_month(issue_date: &str) -> DbResult<(String, String)> {
    let t = issue_date.trim();
    if t.len() < 7 || !t.as_bytes().get(4).copied().eq(&Some(b'-')) {
        return Err(format!(
            "Date d’émission invalide « {issue_date} » (attendu AAAA-MM-JJ)"
        )
        .into());
    }
    let year = &t[0..4];
    let month = &t[5..7];
    if !year.chars().all(|c| c.is_ascii_digit())
        || !month.chars().all(|c| c.is_ascii_digit())
    {
        return Err(format!(
            "Date d’émission invalide « {issue_date} » (attendu AAAA-MM-JJ)"
        )
        .into());
    }
    let m: u32 = month
        .parse()
        .map_err(|_| format!("Mois invalide dans la date « {issue_date} »"))?;
    if !(1..=12).contains(&m) {
        return Err(format!("Mois invalide dans la date « {issue_date} »").into());
    }
    Ok((year.to_string(), format!("{m:02}")))
}

fn preview_month_now() -> String {
    Utc::now().format("%m").to_string()
}

/// Extrait le compteur annuel d’un numéro pour l’année civile donnée.
/// Accepte `F2026-1101` / `D2026-1101`, l’ancien `AAAA-MM-NNNN`, et `PREFIX-AAAA-NNNN`.
fn parse_sequence_num_for_year(number: &str, kind: &str, calendar_year: &str) -> Option<i64> {
    let number = number.trim();
    let letter = document_number_letter(kind);

    // Actuel : F2026-1101 / D2026-1101 (mois collé + séquence ≥ 2 chiffres)
    let glued_prefix = format!("{letter}{calendar_year}-");
    if let Some(rest) = number.strip_prefix(&glued_prefix) {
        if rest.len() >= 3 && rest.chars().all(|c| c.is_ascii_digit()) {
            let month = &rest[0..2];
            let seq = &rest[2..];
            let m: u32 = month.parse().ok()?;
            if (1..=12).contains(&m) && !seq.is_empty() {
                return seq.parse().ok();
            }
        }
    }

    // 0.6.8 : 2026-11-0001 (sans lettre, tiret entre mois et séquence)
    let dashed_prefix = format!("{calendar_year}-");
    if let Some(rest) = number.strip_prefix(&dashed_prefix) {
        let mut parts = rest.split('-');
        let month = parts.next()?;
        let seq = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        if month.len() == 2
            && month.chars().all(|c| c.is_ascii_digit())
            && seq.chars().all(|c| c.is_ascii_digit())
        {
            return seq.parse().ok();
        }
    }

    // Legacy : FACT-2026-0042 / DEV-2026-0001 / ACMP-… / AVO-…
    let legacy_kinds: &[&str] = if kind == "DEV" {
        &["DEV"]
    } else {
        &["FACT", "ACMP", "AVO"]
    };
    for lk in legacy_kinds {
        let prefix = format!("{lk}-{calendar_year}-");
        if let Some(suf) = number.strip_prefix(&prefix) {
            if suf.chars().all(|c| c.is_ascii_digit()) {
                return suf.parse().ok();
            }
        }
    }
    None
}

fn numbers_table_for_kind(kind: &str) -> &'static str {
    if kind == "DEV" {
        "quote"
    } else {
        "invoice"
    }
}

/// Plus grand suffixe numérique déjà utilisé pour cette série / année civile (0 si aucun).
pub fn max_used_document_num(
    conn: &Connection,
    kind: &str,
    calendar_year: &str,
) -> DbResult<i64> {
    let table = numbers_table_for_kind(kind);
    let sql = format!("SELECT number FROM {table} WHERE number IS NOT NULL");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut max = 0i64;
    for row in rows {
        let number = row?;
        if let Some(n) = parse_sequence_num_for_year(&number, kind, calendar_year) {
            if n > max {
                max = n;
            }
        }
    }
    Ok(max)
}

fn migrate_sequence_cal_from_legacy(conn: &Connection) -> DbResult<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM document_sequence_cal",
        [],
        |r| r.get(0),
    )?;
    if count > 0 {
        return Ok(());
    }
    conn.execute(
        "INSERT OR IGNORE INTO document_sequence_cal(kind, calendar_year, next_num)
         SELECT ds.kind, fy.label, ds.next_num
         FROM document_sequence ds
         JOIN fiscal_year fy ON fy.id = ds.fiscal_year_id
         WHERE ds.kind IN ('FACT', 'DEV')",
        [],
    )?;
    // Reprendre le max des anciennes séries acompte / avoir dans FACT.
    for legacy in ["ACMP", "AVO"] {
        let pairs: Vec<(String, i64)> = {
            let mut stmt = conn.prepare(
                "SELECT fy.label, ds.next_num
                 FROM document_sequence ds
                 JOIN fiscal_year fy ON fy.id = ds.fiscal_year_id
                 WHERE ds.kind = ?1",
            )?;
            let mapped = stmt.query_map(params![legacy], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            mapped.filter_map(|r| r.ok()).collect()
        };
        for (year, next_num) in pairs {
            let current: i64 = conn
                .query_row(
                    "SELECT next_num FROM document_sequence_cal
                     WHERE kind = 'FACT' AND calendar_year = ?1",
                    params![year],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or(1);
            if next_num > current {
                conn.execute(
                    "INSERT INTO document_sequence_cal(kind, calendar_year, next_num)
                     VALUES ('FACT', ?1, ?2)
                     ON CONFLICT(kind, calendar_year)
                     DO UPDATE SET next_num = excluded.next_num",
                    params![year, next_num],
                )?;
            }
        }
    }
    Ok(())
}

fn read_stored_next_num(
    conn: &Connection,
    kind: &str,
    calendar_year: &str,
) -> DbResult<i64> {
    migrate_sequence_cal_from_legacy(conn)?;
    Ok(conn
        .query_row(
            "SELECT next_num FROM document_sequence_cal
             WHERE kind = ?1 AND calendar_year = ?2",
            params![kind, calendar_year],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(1))
}

fn upsert_next_num(
    conn: &Connection,
    kind: &str,
    calendar_year: &str,
    next_num: i64,
) -> DbResult<()> {
    if next_num < 1 {
        return Err("Le prochain numéro doit être ≥ 1".into());
    }
    migrate_sequence_cal_from_legacy(conn)?;
    conn.execute(
        "INSERT INTO document_sequence_cal(kind, calendar_year, next_num)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(kind, calendar_year) DO UPDATE SET next_num = excluded.next_num",
        params![kind, calendar_year, next_num],
    )?;
    Ok(())
}

fn document_number_exists(conn: &Connection, kind: &str, number: &str) -> DbResult<bool> {
    let table = numbers_table_for_kind(kind);
    let sql = format!("SELECT 1 FROM {table} WHERE number = ?1 LIMIT 1");
    let found: Option<i64> = conn
        .query_row(&sql, params![number], |r| r.get(0))
        .optional()?;
    Ok(found.is_some())
}

/// Réserve atomiquement le prochain numéro (`F2026-1101` / `D2026-1101`…).
/// Compteur annuel calendaire (depuis le 01/01) ; MM = mois de la date d’émission.
pub fn allocate_document_number(
    conn: &Connection,
    kind: &str,
    issue_date: &str,
) -> DbResult<String> {
    let (year, month) = parse_issue_year_month(issue_date)?;
    let stored = read_stored_next_num(conn, kind, &year)?;
    let max_used = max_used_document_num(conn, kind, &year)?;
    let mut next = stored.max(max_used + 1).max(1);

    // Garde-fou collision (numéros hors format ou importés).
    for _ in 0..10_000 {
        let candidate = format_document_number(kind, &year, &month, next);
        if !document_number_exists(conn, kind, &candidate)? {
            upsert_next_num(conn, kind, &year, next + 1)?;
            return Ok(candidate);
        }
        next += 1;
    }
    Err("Impossible d’allouer un numéro libre pour cette série".into())
}

fn sequence_info_for(
    conn: &Connection,
    kind: &str,
    fiscal_year_id: i64,
    calendar_year: &str,
) -> DbResult<DocumentSequenceInfo> {
    let max_used = max_used_document_num(conn, kind, calendar_year)?;
    let min_next = (max_used + 1).max(1);
    let stored = read_stored_next_num(conn, kind, calendar_year)?;
    let next_num = stored.max(min_next);
    let month = preview_month_now();
    Ok(DocumentSequenceInfo {
        kind: kind.to_string(),
        label: sequence_label_fr(kind),
        fiscal_year_id,
        fiscal_year_label: calendar_year.to_string(),
        next_num,
        min_next_num: min_next,
        preview: format_document_number(kind, calendar_year, &month, next_num),
    })
}

/// Compteurs de l’année civile de l’exercice ouvert (FACT / DEV).
pub fn list_document_sequences(conn: &Connection) -> DbResult<Vec<DocumentSequenceInfo>> {
    let fy_id = open_fy_id(conn)?;
    let year = calendar_year_for_open_fy(conn, fy_id)?;
    let mut out = Vec::with_capacity(DOCUMENT_SEQUENCE_KINDS.len());
    for kind in DOCUMENT_SEQUENCE_KINDS {
        out.push(sequence_info_for(conn, kind, fy_id, &year)?);
    }
    Ok(out)
}

/// Définit le prochain numéro d’une ou plusieurs séries (année civile de l’exercice ouvert).
pub fn set_document_sequences(
    conn: &Connection,
    inputs: &[SetDocumentSequenceInput],
) -> DbResult<Vec<DocumentSequenceInfo>> {
    if inputs.is_empty() {
        return Err("Aucun compteur à enregistrer".into());
    }
    let fy_id = open_fy_id(conn)?;
    let year = calendar_year_for_open_fy(conn, fy_id)?;
    let month = preview_month_now();
    for input in inputs {
        let kind = input.kind.trim();
        validate_sequence_kind(kind)?;
        if input.next_num < 1 {
            return Err(format!(
                "Série {kind} : le prochain numéro doit être un entier ≥ 1"
            )
            .into());
        }
        let max_used = max_used_document_num(conn, kind, &year)?;
        let min_next = (max_used + 1).max(1);
        if input.next_num < min_next {
            let conflict = format_document_number(kind, &year, &month, max_used);
            return Err(format!(
                "Série {kind} : le prochain numéro doit être ≥ {min_next} \
                 (un document de l’année {year} porte déjà le compteur {max_used}, \
                 ex. {conflict}). Indiquez le numéro suivant votre dernière \
                 pièce hors SasCompta."
            )
            .into());
        }
        let candidate = format_document_number(kind, &year, &month, input.next_num);
        if document_number_exists(conn, kind, &candidate)? {
            return Err(format!(
                "Série {kind} : le numéro {candidate} est déjà attribué. \
                 Choisissez un prochain numéro plus élevé."
            )
            .into());
        }
        upsert_next_num(conn, kind, &year, input.next_num)?;
    }
    list_document_sequences(conn)
}

pub fn compute_line_ht(quantity_milli: i64, unit_price_cents: i64) -> i64 {
    // qty (milli) * prix unitaire / 1000, arrondi au centime
    ((quantity_milli as i128) * (unit_price_cents as i128) / 1000) as i64
}

pub fn compute_vat_cents(ht_cents: i64, rate_bp: i32) -> i64 {
    if ht_cents <= 0 || rate_bp <= 0 {
        return 0;
    }
    (((ht_cents as i128) * (rate_bp as i128) + 5_000) / 10_000) as i64
}

pub fn legal_mentions_for(vat_mode: &str) -> String {
    match vat_mode {
        "autoliquidation_chantier" => {
            "TVA due par le preneur — autoliquidation (art. 283 du CGI). Montant HT facturé ; \
             le client assujetti reverse la TVA."
                .into()
        }
        "sans" => "Opération sans TVA (franchise en base / exonération / hors champ).".into(),
        _ => String::new(),
    }
}

fn effective_vat_for_line(vat_mode: &str, ht: i64, rate_bp: i32) -> i64 {
    match vat_mode {
        "normale" => compute_vat_cents(ht, rate_bp),
        // chantier / sans : pas de TVA sur le document
        _ => 0,
    }
}

fn recompute_quote_totals(
    lines: &[QuoteLineInput],
    vat_mode: &str,
) -> (i64, i64, i64, Vec<(QuoteLineInput, i64)>) {
    let mut ht_total = 0i64;
    let mut vat_total = 0i64;
    let mut computed = Vec::new();
    for line in lines {
        let ht = if line.ht_cents != 0 {
            line.ht_cents
        } else {
            compute_line_ht(line.quantity_milli, line.unit_price_cents)
        };
        let vat = effective_vat_for_line(vat_mode, ht, line.vat_rate_bp);
        ht_total += ht;
        vat_total += vat;
        let mut l = line.clone();
        l.ht_cents = ht;
        computed.push((l, vat));
    }
    (ht_total, vat_total, ht_total + vat_total, computed)
}

fn map_quote_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Quote> {
    Ok(Quote {
        id: r.get(0)?,
        number: r.get(1)?,
        third_party_id: r.get(2)?,
        third_party_code: r.get(3)?,
        third_party_name: r.get(4)?,
        status: r.get(5)?,
        issue_date: r.get(6)?,
        valid_until: r.get(7)?,
        currency: r.get(8)?,
        ht_cents: r.get(9)?,
        vat_cents: r.get(10)?,
        ttc_cents: r.get(11)?,
        vat_mode: r.get(12)?,
        notes: r.get(13)?,
        legal_mentions: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
        lines: Vec::new(),
    })
}

fn load_quote_lines(conn: &Connection, quote_id: i64) -> DbResult<Vec<QuoteLine>> {
    let mut stmt = conn.prepare(
        "SELECT id, quote_id, line_no, label, description, quantity_milli, unit, unit_price_cents,
                ht_cents, vat_rate_bp, vat_code, account_number
         FROM quote_line WHERE quote_id = ?1 ORDER BY line_no",
    )?;
    let rows = stmt.query_map(params![quote_id], |r| {
        Ok(QuoteLine {
            id: r.get(0)?,
            quote_id: r.get(1)?,
            line_no: r.get(2)?,
            label: r.get(3)?,
            description: r.get(4)?,
            quantity_milli: r.get(5)?,
            unit: r.get(6)?,
            unit_price_cents: r.get(7)?,
            ht_cents: r.get(8)?,
            vat_rate_bp: r.get(9)?,
            vat_code: r.get(10)?,
            account_number: r.get(11)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn get_quote_header(conn: &Connection, id: i64) -> DbResult<Quote> {
    conn.query_row(
        "SELECT q.id, q.number, q.third_party_id, tp.code, tp.name, q.status,
                q.issue_date, q.valid_until, q.currency, q.ht_cents, q.vat_cents, q.ttc_cents,
                q.vat_mode, q.notes, q.legal_mentions, q.created_at, q.updated_at
         FROM quote q
         JOIN third_party tp ON tp.id = q.third_party_id
         WHERE q.id = ?1",
        params![id],
        map_quote_row,
    )
    .map_err(|_| "Devis introuvable".into())
}

pub fn get_quote(conn: &Connection, id: i64) -> DbResult<Quote> {
    let mut q = get_quote_header(conn, id)?;
    q.lines = load_quote_lines(conn, id)?;
    Ok(q)
}

pub fn list_quotes(conn: &Connection) -> DbResult<Vec<Quote>> {
    let mut stmt = conn.prepare(
        "SELECT q.id, q.number, q.third_party_id, tp.code, tp.name, q.status,
                q.issue_date, q.valid_until, q.currency, q.ht_cents, q.vat_cents, q.ttc_cents,
                q.vat_mode, q.notes, q.legal_mentions, q.created_at, q.updated_at
         FROM quote q
         JOIN third_party tp ON tp.id = q.third_party_id
         ORDER BY q.issue_date DESC, q.id DESC",
    )?;
    let rows = stmt.query_map([], map_quote_row)?;
    let mut out = Vec::new();
    for row in rows {
        let mut q = row?;
        q.lines = load_quote_lines(conn, q.id)?;
        out.push(q);
    }
    Ok(out)
}

fn ensure_customer(conn: &Connection, third_party_id: i64) -> DbResult<()> {
    let kind: String = conn
        .query_row(
            "SELECT kind FROM third_party WHERE id = ?1 AND active = 1",
            params![third_party_id],
            |r| r.get(0),
        )
        .map_err(|_| "Client introuvable ou inactif")?;
    if kind != "customer" {
        return Err("Le devis doit cibler un tiers client".into());
    }
    Ok(())
}

fn validate_vat_mode(mode: &str) -> DbResult<()> {
    match mode {
        "normale" | "sans" | "autoliquidation_chantier" => Ok(()),
        _ => Err("Régime TVA devis invalide".into()),
    }
}

fn normalize_description(raw: &Option<String>) -> Option<String> {
    raw.as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn insert_quote_lines(
    conn: &Connection,
    quote_id: i64,
    lines: &[(QuoteLineInput, i64)],
) -> DbResult<()> {
    for (i, (line, _)) in lines.iter().enumerate() {
        let label = line.label.trim();
        if label.is_empty() {
            return Err("Titre de ligne requis".into());
        }
        if line.quantity_milli <= 0 {
            return Err("Quantité invalide".into());
        }
        let account = if line.account_number.trim().is_empty() {
            "706"
        } else {
            line.account_number.trim()
        };
        let description = normalize_description(&line.description);
        conn.execute(
            "INSERT INTO quote_line(
                quote_id, line_no, label, description, quantity_milli, unit, unit_price_cents,
                ht_cents, vat_rate_bp, vat_code, account_number
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                quote_id,
                (i + 1) as i32,
                label,
                description,
                line.quantity_milli,
                if line.unit.trim().is_empty() {
                    "u"
                } else {
                    line.unit.trim()
                },
                line.unit_price_cents,
                line.ht_cents,
                line.vat_rate_bp,
                line.vat_code,
                account,
            ],
        )?;
    }
    Ok(())
}

pub fn create_quote(conn: &Connection, input: &CreateQuoteInput) -> DbResult<Quote> {
    ensure_customer(conn, input.third_party_id)?;
    validate_vat_mode(&input.vat_mode)?;
    if input.lines.is_empty() {
        return Err("Ajoutez au moins une ligne au devis".into());
    }
    let number = allocate_document_number(conn, "DEV", &input.issue_date)?;
    let (ht, vat, ttc, lines) = recompute_quote_totals(&input.lines, &input.vat_mode);
    let now = now_iso();
    let legal = input
        .legal_mentions
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| legal_mentions_for(&input.vat_mode));
    conn.execute(
        "INSERT INTO quote(
            number, third_party_id, status, issue_date, valid_until, currency,
            ht_cents, vat_cents, ttc_cents, vat_mode, notes, legal_mentions,
            created_at, updated_at
         ) VALUES (?1, ?2, 'draft', ?3, ?4, 'EUR', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
        params![
            number,
            input.third_party_id,
            input.issue_date,
            input.valid_until,
            ht,
            vat,
            ttc,
            input.vat_mode,
            input.notes,
            if legal.is_empty() { None } else { Some(legal) },
            now,
        ],
    )?;
    let id = conn.last_insert_rowid();
    insert_quote_lines(conn, id, &lines)?;
    get_quote(conn, id)
}

pub fn update_quote(conn: &Connection, id: i64, input: &CreateQuoteInput) -> DbResult<Quote> {
    let existing = get_quote_header(conn, id)?;
    if existing.status != "draft" && existing.status != "sent" {
        return Err(
            "Seuls les devis brouillon ou envoyés restent modifiables (lignes / montants)".into(),
        );
    }
    ensure_customer(conn, input.third_party_id)?;
    validate_vat_mode(&input.vat_mode)?;
    if input.lines.is_empty() {
        return Err("Ajoutez au moins une ligne au devis".into());
    }
    let (ht, vat, ttc, lines) = recompute_quote_totals(&input.lines, &input.vat_mode);
    let now = now_iso();
    let legal = input
        .legal_mentions
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| legal_mentions_for(&input.vat_mode));
    conn.execute(
        "UPDATE quote SET third_party_id = ?1, issue_date = ?2, valid_until = ?3,
            ht_cents = ?4, vat_cents = ?5, ttc_cents = ?6, vat_mode = ?7,
            notes = ?8, legal_mentions = ?9, updated_at = ?10
         WHERE id = ?11",
        params![
            input.third_party_id,
            input.issue_date,
            input.valid_until,
            ht,
            vat,
            ttc,
            input.vat_mode,
            input.notes,
            if legal.is_empty() { None } else { Some(legal) },
            now,
            id,
        ],
    )?;
    conn.execute("DELETE FROM quote_line WHERE quote_id = ?1", params![id])?;
    insert_quote_lines(conn, id, &lines)?;
    get_quote(conn, id)
}

pub fn set_quote_status(conn: &Connection, id: i64, status: &str) -> DbResult<Quote> {
    if !QUOTE_STATUSES.contains(&status) {
        return Err("Statut de devis invalide".into());
    }
    let existing = get_quote_header(conn, id)?;
    let allowed = match existing.status.as_str() {
        "draft" => matches!(status, "sent" | "refused" | "expired"),
        "sent" => matches!(status, "accepted" | "refused" | "expired" | "draft"),
        "accepted" => matches!(status, "invoiced" | "expired"),
        "refused" | "expired" => false,
        "invoiced" => false,
        _ => false,
    };
    // allow direct draft→accepted for UX shortcuts
    let allowed = allowed
        || (existing.status == "draft" && matches!(status, "accepted" | "sent"))
        || (existing.status == "sent" && status == "accepted");
    if !allowed {
        return Err(format!(
            "Transition de statut interdite : {} → {}",
            existing.status, status
        )
        .into());
    }
    if status == "invoiced" {
        return Err("Passez par la conversion devis → facture".into());
    }
    conn.execute(
        "UPDATE quote SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, now_iso(), id],
    )?;
    get_quote(conn, id)
}

pub fn delete_quote(conn: &Connection, id: i64) -> DbResult<()> {
    let existing = get_quote_header(conn, id)?;
    if existing.status != "draft" {
        return Err("Seuls les devis brouillon peuvent être supprimés".into());
    }
    let n = conn.execute("DELETE FROM quote WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err("Devis introuvable".into());
    }
    Ok(())
}

fn map_invoice_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Invoice> {
    Ok(Invoice {
        id: r.get(0)?,
        number: r.get(1)?,
        kind: r.get(2)?,
        third_party_id: r.get(3)?,
        third_party_code: r.get(4)?,
        third_party_name: r.get(5)?,
        quote_id: r.get(6)?,
        parent_invoice_id: r.get(7)?,
        status: r.get(8)?,
        issue_date: r.get(9)?,
        due_date: r.get(10)?,
        service_date: r.get(11)?,
        ht_cents: r.get(12)?,
        vat_cents: r.get(13)?,
        ttc_cents: r.get(14)?,
        paid_cents: r.get(15)?,
        vat_mode: r.get(16)?,
        vat_code: r.get(17)?,
        einvoice_profile: r.get(18)?,
        einvoice_status: r.get(19)?,
        entry_id: r.get(20)?,
        pdf_path: r.get(21)?,
        facturx_path: r.get(22)?,
        notes: r.get(23)?,
        legal_mentions: r.get(24)?,
        purchase_order_number: r.get(25)?,
        created_at: r.get(26)?,
        updated_at: r.get(27)?,
        source: r.get::<_, Option<String>>(28)?.unwrap_or_else(|| "native".into()),
        document_import_id: r.get(29)?,
        lines: Vec::new(),
    })
}

fn load_invoice_lines(conn: &Connection, invoice_id: i64) -> DbResult<Vec<InvoiceLine>> {
    let mut stmt = conn.prepare(
        "SELECT id, invoice_id, line_no, label, description, quantity_milli, unit, unit_price_cents,
                ht_cents, vat_rate_bp, vat_code, account_number, analytic_code
         FROM invoice_line WHERE invoice_id = ?1 ORDER BY line_no",
    )?;
    let rows = stmt.query_map(params![invoice_id], |r| {
        Ok(InvoiceLine {
            id: r.get(0)?,
            invoice_id: r.get(1)?,
            line_no: r.get(2)?,
            label: r.get(3)?,
            description: r.get(4)?,
            quantity_milli: r.get(5)?,
            unit: r.get(6)?,
            unit_price_cents: r.get(7)?,
            ht_cents: r.get(8)?,
            vat_rate_bp: r.get(9)?,
            vat_code: r.get(10)?,
            account_number: r.get(11)?,
            analytic_code: r.get(12)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

const INVOICE_SELECT: &str = "SELECT i.id, i.number, i.kind, i.third_party_id, tp.code, tp.name,
                i.quote_id, i.parent_invoice_id, i.status, i.issue_date, i.due_date,
                i.service_date, i.ht_cents, i.vat_cents, i.ttc_cents, i.paid_cents,
                i.vat_mode, i.vat_code, i.einvoice_profile, i.einvoice_status,
                i.entry_id, i.pdf_path, i.facturx_path, i.notes, i.legal_mentions,
                i.purchase_order_number, i.created_at, i.updated_at, i.source, i.document_import_id
         FROM invoice i
         JOIN third_party tp ON tp.id = i.third_party_id";

pub fn get_invoice(conn: &Connection, id: i64) -> DbResult<Invoice> {
    let mut inv = conn
        .query_row(
            &format!("{INVOICE_SELECT} WHERE i.id = ?1"),
            params![id],
            map_invoice_row,
        )
        .map_err(|_| "Facture introuvable")?;
    inv.lines = load_invoice_lines(conn, id)?;
    Ok(inv)
}

pub fn list_invoices(conn: &Connection) -> DbResult<Vec<Invoice>> {
    let mut stmt = conn.prepare(&format!(
        "{INVOICE_SELECT} ORDER BY i.issue_date DESC, i.id DESC"
    ))?;
    let rows = stmt.query_map([], map_invoice_row)?;
    let mut out = Vec::new();
    for row in rows {
        let mut inv = row?;
        inv.lines = load_invoice_lines(conn, inv.id)?;
        out.push(inv);
    }
    Ok(out)
}

fn sequence_kind_for_invoice(_kind: &str) -> &'static str {
    // Une seule série annuelle pour toutes les factures (format FAAAA-MMNN).
    "FACT"
}

fn validate_invoice_kind(kind: &str) -> DbResult<()> {
    if INVOICE_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err("Type de facture invalide (standard|deposit|balance|credit_note)".into())
    }
}

fn default_account_for_kind(kind: &str) -> &'static str {
    match kind {
        "deposit" => "4191",
        _ => "706",
    }
}

fn recompute_invoice_totals(
    lines: &[InvoiceLineInput],
    vat_mode: &str,
) -> (i64, i64, i64, Vec<InvoiceLineInput>) {
    let mut ht_total = 0i64;
    let mut vat_total = 0i64;
    let mut computed = Vec::new();
    for line in lines {
        let ht = if line.ht_cents != 0 {
            line.ht_cents
        } else {
            compute_line_ht(line.quantity_milli, line.unit_price_cents)
        };
        let vat = effective_vat_for_line(vat_mode, ht, line.vat_rate_bp);
        ht_total += ht;
        vat_total += vat;
        let mut l = line.clone();
        l.ht_cents = ht;
        if l.account_number.trim().is_empty() {
            l.account_number = "706".into();
        }
        computed.push(l);
    }
    (ht_total, vat_total, ht_total + vat_total, computed)
}

fn insert_invoice_lines(
    conn: &Connection,
    invoice_id: i64,
    lines: &[InvoiceLineInput],
) -> DbResult<()> {
    for (i, line) in lines.iter().enumerate() {
        let label = line.label.trim();
        if label.is_empty() {
            return Err("Titre de ligne requis".into());
        }
        if line.quantity_milli <= 0 {
            return Err("Quantité invalide".into());
        }
        let description = normalize_description(&line.description);
        conn.execute(
            "INSERT INTO invoice_line(
                invoice_id, line_no, label, description, quantity_milli, unit, unit_price_cents,
                ht_cents, vat_rate_bp, vat_code, account_number, analytic_code
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                invoice_id,
                (i + 1) as i32,
                label,
                description,
                line.quantity_milli,
                if line.unit.trim().is_empty() {
                    "u"
                } else {
                    line.unit.trim()
                },
                line.unit_price_cents,
                line.ht_cents,
                line.vat_rate_bp,
                line.vat_code,
                line.account_number.trim(),
                line.analytic_code,
            ],
        )?;
    }
    Ok(())
}

pub fn create_invoice(conn: &Connection, input: &CreateInvoiceInput) -> DbResult<Invoice> {
    validate_invoice_kind(&input.kind)?;
    ensure_customer(conn, input.third_party_id)?;
    validate_vat_mode(&input.vat_mode)?;
    if input.lines.is_empty() {
        return Err("Ajoutez au moins une ligne à la facture".into());
    }
    if input.kind == "credit_note" && input.parent_invoice_id.is_none() {
        return Err("Un avoir doit référencer la facture d'origine".into());
    }
    if input.kind == "balance" && input.parent_invoice_id.is_none() {
        return Err("Une facture de solde doit référencer le marché / acompte parent".into());
    }
    let (ht, vat, ttc, lines) = recompute_invoice_totals(&input.lines, &input.vat_mode);
    let now = now_iso();
    let legal = input
        .legal_mentions
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| legal_mentions_for(&input.vat_mode));
    let vat_code = lines.iter().find_map(|l| l.vat_code.clone());
    let po = input
        .purchase_order_number
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    conn.execute(
        "INSERT INTO invoice(
            number, kind, third_party_id, quote_id, parent_invoice_id, status,
            issue_date, due_date, service_date, ht_cents, vat_cents, ttc_cents, paid_cents,
            vat_mode, vat_code, notes, legal_mentions, purchase_order_number, created_at, updated_at
         ) VALUES (NULL, ?1, ?2, ?3, ?4, 'draft', ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, ?12, ?13, ?14, ?15, ?16, ?16)",
        params![
            input.kind,
            input.third_party_id,
            input.quote_id,
            input.parent_invoice_id,
            input.issue_date,
            input.due_date,
            input.service_date,
            ht,
            vat,
            ttc,
            input.vat_mode,
            vat_code,
            input.notes,
            if legal.is_empty() { None } else { Some(legal) },
            po,
            now,
        ],
    )?;
    let id = conn.last_insert_rowid();
    insert_invoice_lines(conn, id, &lines)?;
    get_invoice(conn, id)
}

pub fn update_invoice(
    conn: &Connection,
    id: i64,
    input: &CreateInvoiceInput,
) -> DbResult<Invoice> {
    let existing = get_invoice(conn, id)?;
    if existing.status != "draft" {
        return Err("Seules les factures brouillon sont modifiables".into());
    }
    validate_invoice_kind(&input.kind)?;
    ensure_customer(conn, input.third_party_id)?;
    validate_vat_mode(&input.vat_mode)?;
    if input.lines.is_empty() {
        return Err("Ajoutez au moins une ligne à la facture".into());
    }
    let (ht, vat, ttc, lines) = recompute_invoice_totals(&input.lines, &input.vat_mode);
    let now = now_iso();
    let legal = input
        .legal_mentions
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| legal_mentions_for(&input.vat_mode));
    let vat_code = lines.iter().find_map(|l| l.vat_code.clone());
    let po = input
        .purchase_order_number
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    conn.execute(
        "UPDATE invoice SET kind = ?1, third_party_id = ?2, quote_id = ?3, parent_invoice_id = ?4,
            issue_date = ?5, due_date = ?6, service_date = ?7,
            ht_cents = ?8, vat_cents = ?9, ttc_cents = ?10, vat_mode = ?11, vat_code = ?12,
            notes = ?13, legal_mentions = ?14, purchase_order_number = ?15, updated_at = ?16
         WHERE id = ?17",
        params![
            input.kind,
            input.third_party_id,
            input.quote_id,
            input.parent_invoice_id,
            input.issue_date,
            input.due_date,
            input.service_date,
            ht,
            vat,
            ttc,
            input.vat_mode,
            vat_code,
            input.notes,
            if legal.is_empty() { None } else { Some(legal) },
            po,
            now,
            id,
        ],
    )?;
    conn.execute("DELETE FROM invoice_line WHERE invoice_id = ?1", params![id])?;
    insert_invoice_lines(conn, id, &lines)?;
    get_invoice(conn, id)
}

pub fn delete_invoice(conn: &Connection, id: i64) -> DbResult<()> {
    let existing = get_invoice(conn, id)?;
    if existing.status != "draft" {
        return Err("Seules les factures brouillon peuvent être supprimées".into());
    }
    if let Some(entry_id) = existing.entry_id {
        let status: String = conn.query_row(
            "SELECT status FROM entry WHERE id = ?1",
            params![entry_id],
            |r| r.get(0),
        )?;
        if status == "posted" {
            return Err("Écriture validée liée — suppression interdite".into());
        }
        conn.execute(
            "UPDATE invoice SET entry_id = NULL WHERE id = ?1",
            params![id],
        )?;
        let _ = ledger::delete_draft(conn, entry_id);
    }
    let n = conn.execute("DELETE FROM invoice WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err("Facture introuvable".into());
    }
    Ok(())
}

/// Émet la facture : réserve le numéro (`FAAAA-MMNN`) + statut issued + brouillon VTE.
///
/// Atomique : si le brouillon VTE échoue, numéro + statut `issued` sont annulés
/// (pas de facture numérotée orpheline sans écriture).
pub fn issue_invoice(conn: &Connection, id: i64) -> DbResult<Invoice> {
    let existing = get_invoice(conn, id)?;
    if existing.status != "draft" {
        return Err("Seules les factures brouillon peuvent être émises".into());
    }
    if existing.lines.is_empty() {
        return Err("Facture sans lignes".into());
    }
    if existing.number.is_some() {
        return Err("Numéro déjà attribué".into());
    }
    let seq = sequence_kind_for_invoice(&existing.kind);
    let tx = conn.unchecked_transaction()?;
    let number = allocate_document_number(&tx, seq, &existing.issue_date)?;
    let now = now_iso();
    tx.execute(
        "UPDATE invoice SET number = ?1, status = 'issued', updated_at = ?2 WHERE id = ?3",
        params![number, now, id],
    )?;
    // Max automation : brouillon VTE dès l'émission (post humain ensuite)
    build_invoice_entry_draft(&tx, id)?;
    tx.commit()?;
    get_invoice(conn, id)
}

/// Numéro externe (logiciel tiers) pour une facture importée — ne consomme pas la série FACT.
fn resolve_external_invoice_number(
    conn: &Connection,
    piece_ref: &str,
    document_import_id: i64,
) -> DbResult<String> {
    let base = piece_ref.trim();
    let candidate = if base.is_empty() {
        format!("IMP-{document_import_id}")
    } else {
        base.to_string()
    };
    let taken: bool = conn
        .query_row(
            "SELECT 1 FROM invoice WHERE number = ?1 LIMIT 1",
            params![candidate],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !taken {
        return Ok(candidate);
    }
    let alt = format!("{candidate}·imp{document_import_id}");
    let alt_taken: bool = conn
        .query_row(
            "SELECT 1 FROM invoice WHERE number = ?1 LIMIT 1",
            params![alt],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if alt_taken {
        return Err(format!(
            "Numéro de facture « {candidate} » déjà utilisé — renommez la pièce"
        )
        .into());
    }
    Ok(alt)
}

/// Crée une facture **émise** liée à une écriture VTE déjà produite par l’import client.
/// Conserve le numéro externe (`piece_ref`) — pas d’allocation série FACT, pas de 2ᵉ VTE.
pub fn create_imported_customer_invoice(
    conn: &Connection,
    document_import_id: i64,
    third_party_id: i64,
    entry_id: i64,
    issue_date: &str,
    piece_ref: &str,
    label: &str,
    ht_cents: i64,
    vat_cents: i64,
    rate_bp: i32,
    vat_mode: &str,
    vat_code: &str,
    ht_account: &str,
    pdf_path: Option<&str>,
) -> DbResult<Invoice> {
    ensure_customer(conn, third_party_id)?;
    validate_vat_mode(vat_mode)?;
    if ht_cents <= 0 {
        return Err("Montant HT invalide pour la facture importée".into());
    }
    let vat = vat_cents.max(0);
    let ttc = ht_cents + vat;
    let number = resolve_external_invoice_number(conn, piece_ref, document_import_id)?;
    let account = if ht_account.trim().is_empty() {
        "706".to_string()
    } else {
        ht_account.trim().to_string()
    };
    let line_label = {
        let t = label.trim();
        if t.is_empty() {
            format!("Import {number}")
        } else {
            t.to_string()
        }
    };
    let notes = format!("Import document #{document_import_id}");
    let legal = legal_mentions_for(vat_mode);
    let now = now_iso();
    let vat_code_opt = if vat_code.trim().is_empty() {
        None
    } else {
        Some(vat_code.trim().to_string())
    };
    let line_vat_code = vat_code_opt.clone();
    conn.execute(
        "INSERT INTO invoice(
            number, kind, third_party_id, quote_id, parent_invoice_id, status,
            issue_date, due_date, service_date, ht_cents, vat_cents, ttc_cents, paid_cents,
            vat_mode, vat_code, notes, legal_mentions, entry_id, pdf_path,
            created_at, updated_at, source, document_import_id
         ) VALUES (?1, 'standard', ?2, NULL, NULL, 'issued', ?3, NULL, NULL, ?4, ?5, ?6, 0,
            ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13, 'import', ?14)",
        params![
            number,
            third_party_id,
            issue_date,
            ht_cents,
            vat,
            ttc,
            vat_mode,
            vat_code_opt,
            notes,
            if legal.is_empty() { None } else { Some(legal) },
            entry_id,
            pdf_path,
            now,
            document_import_id,
        ],
    )?;
    let id = conn.last_insert_rowid();
    let line = InvoiceLineInput {
        label: line_label,
        description: Some("Facture client importée (autre logiciel)".into()),
        quantity_milli: 1000,
        unit: "u".into(),
        unit_price_cents: ht_cents,
        ht_cents,
        vat_rate_bp: rate_bp.max(0),
        vat_code: line_vat_code,
        account_number: account,
        analytic_code: None,
    };
    insert_invoice_lines(conn, id, &[line])?;
    get_invoice(conn, id)
}

/// Avoir brouillon inversant une facture émise (numéro AVO à l'émission).
pub fn create_credit_note(conn: &Connection, invoice_id: i64) -> DbResult<Invoice> {
    let src = get_invoice(conn, invoice_id)?;
    if !matches!(src.status.as_str(), "issued" | "sent" | "partially_paid" | "paid") {
        return Err("L'avoir nécessite une facture déjà émise".into());
    }
    if src.kind == "credit_note" {
        return Err("Impossible de créer un avoir sur un avoir".into());
    }
    let lines: Vec<InvoiceLineInput> = src
        .lines
        .iter()
        .map(|l| InvoiceLineInput {
            label: format!("Avoir — {}", l.label),
            description: l.description.clone(),
            quantity_milli: l.quantity_milli,
            unit: l.unit.clone(),
            unit_price_cents: l.unit_price_cents,
            ht_cents: l.ht_cents,
            vat_rate_bp: l.vat_rate_bp,
            vat_code: l.vat_code.clone(),
            account_number: l.account_number.clone(),
            analytic_code: l.analytic_code.clone(),
        })
        .collect();
    let input = CreateInvoiceInput {
        kind: "credit_note".into(),
        third_party_id: src.third_party_id,
        quote_id: None,
        parent_invoice_id: Some(src.id),
        issue_date: src.issue_date.clone(),
        due_date: None,
        service_date: src.service_date.clone(),
        vat_mode: src.vat_mode.clone(),
        notes: Some(format!(
            "Avoir sur {}",
            src.number.as_deref().unwrap_or(&format!("#{}", src.id))
        )),
        legal_mentions: src.legal_mentions.clone(),
        purchase_order_number: src.purchase_order_number.clone(),
        lines,
    };
    create_invoice(conn, &input)
}

/// Acompte brouillon lié à une facture marché (compte 4191 par défaut).
pub fn create_deposit_invoice(
    conn: &Connection,
    parent_invoice_id: i64,
    amount_ht_cents: i64,
    vat_rate_bp: i32,
    vat_code: Option<String>,
) -> DbResult<Invoice> {
    let parent = get_invoice(conn, parent_invoice_id)?;
    if parent.kind == "credit_note" {
        return Err("Pas d'acompte sur un avoir".into());
    }
    if amount_ht_cents <= 0 {
        return Err("Montant d'acompte invalide".into());
    }
    let input = CreateInvoiceInput {
        kind: "deposit".into(),
        third_party_id: parent.third_party_id,
        quote_id: parent.quote_id,
        parent_invoice_id: Some(parent.id),
        issue_date: parent.issue_date.clone(),
        due_date: parent.due_date.clone(),
        service_date: None,
        vat_mode: parent.vat_mode.clone(),
        notes: Some(format!(
            "Acompte sur {}",
            parent.number.as_deref().unwrap_or(&format!("#{}", parent.id))
        )),
        legal_mentions: parent.legal_mentions.clone(),
        purchase_order_number: parent.purchase_order_number.clone(),
        lines: vec![InvoiceLineInput {
            label: "Acompte".into(),
            description: None,
            quantity_milli: 1000,
            unit: "u".into(),
            unit_price_cents: amount_ht_cents,
            ht_cents: amount_ht_cents,
            vat_rate_bp,
            vat_code,
            account_number: default_account_for_kind("deposit").into(),
            analytic_code: None,
        }],
    };
    create_invoice(conn, &input)
}

/// Conversion devis accepté → facture brouillon (numéro réservé à l'émission).
pub fn convert_quote_to_invoice(conn: &Connection, quote_id: i64) -> DbResult<Invoice> {
    // Idempotence d'abord : un devis déjà converti renvoie la facture liée.
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM invoice WHERE quote_id = ?1 LIMIT 1",
            params![quote_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return get_invoice(conn, id);
    }
    let quote = get_quote(conn, quote_id)?;
    if quote.status != "accepted" {
        return Err("Seuls les devis acceptés peuvent être transformés en facture".into());
    }
    let now = now_iso();
    // Échéance par défaut : date devis + 30 j (sinon relances silencieusement absentes).
    let due_date = default_due_date(&quote.issue_date);
    conn.execute(
        "INSERT INTO invoice(
            number, kind, third_party_id, quote_id, status, issue_date, due_date,
            ht_cents, vat_cents, ttc_cents, paid_cents, vat_mode, notes, legal_mentions,
            created_at, updated_at
         ) VALUES (NULL, 'standard', ?1, ?2, 'draft', ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10, ?11, ?11)",
        params![
            quote.third_party_id,
            quote_id,
            quote.issue_date,
            due_date,
            quote.ht_cents,
            quote.vat_cents,
            quote.ttc_cents,
            quote.vat_mode,
            quote.notes,
            quote.legal_mentions,
            now,
        ],
    )?;
    let invoice_id = conn.last_insert_rowid();
    for line in &quote.lines {
        conn.execute(
            "INSERT INTO invoice_line(
                invoice_id, line_no, label, description, quantity_milli, unit, unit_price_cents,
                ht_cents, vat_rate_bp, vat_code, account_number
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                invoice_id,
                line.line_no,
                line.label,
                line.description,
                line.quantity_milli,
                line.unit,
                line.unit_price_cents,
                line.ht_cents,
                line.vat_rate_bp,
                line.vat_code,
                line.account_number,
            ],
        )?;
    }
    conn.execute(
        "UPDATE quote SET status = 'invoiced', updated_at = ?1 WHERE id = ?2",
        params![now, quote_id],
    )?;
    get_invoice(conn, invoice_id)
}

/// Construit (ou remplace) un brouillon VTE lié à la facture — jamais de double post.
pub fn build_invoice_entry_draft(conn: &Connection, invoice_id: i64) -> DbResult<Entry> {
    let inv = get_invoice(conn, invoice_id)?;
    if !INVOICE_KINDS.contains(&inv.kind.as_str()) {
        return Err("Type de facture inconnu".into());
    }
    if matches!(inv.status.as_str(), "cancelled") {
        return Err("Facture annulée : pas d'écriture".into());
    }
    if inv.lines.is_empty() {
        return Err("Facture sans lignes".into());
    }

    let piece = inv
        .number
        .clone()
        .unwrap_or_else(|| format!("FAC-DRAFT-{}", inv.id));
    let label = format!("Vente {piece}");
    let lines = build_vte_lines_from_invoice(&inv)?;
    let input = CreateEntryInput {
        journal_code: "VTE".into(),
        entry_date: inv.issue_date.clone(),
        piece_ref: piece,
        label,
        lines,
    };

    // Idempotence : postée → refus ; draft existant → remplace les lignes (pas de double)
    if let Some(entry_id) = inv.entry_id {
        let status: String = conn.query_row(
            "SELECT status FROM entry WHERE id = ?1",
            params![entry_id],
            |r| r.get(0),
        )?;
        if status == "posted" {
            return Err(
                "Écriture déjà validée — proposez un avoir / contrepassation, ne mutez pas"
                    .into(),
            );
        }
        let entry = ledger::update_draft(conn, entry_id, &input)?;
        conn.execute(
            "UPDATE invoice SET updated_at = ?1 WHERE id = ?2",
            params![now_iso(), invoice_id],
        )?;
        return Ok(entry);
    }

    let entry = ledger::create_draft(conn, &input)?;
    conn.execute(
        "UPDATE invoice SET entry_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![entry.id, now_iso(), invoice_id],
    )?;
    Ok(entry)
}

fn build_vte_lines_from_invoice(inv: &Invoice) -> DbResult<Vec<EntryLineInput>> {
    let reverse = inv.kind == "credit_note";
    let mut lines = Vec::new();
    let mut ht_sum = 0i64;
    for l in &inv.lines {
        ht_sum += l.ht_cents;
        let account = if inv.kind == "deposit" && l.account_number.starts_with('7') {
            default_account_for_kind("deposit").to_string()
        } else {
            l.account_number.clone()
        };
        // Vente : crédit produit / 4191 ; avoir : débit (sens inverse)
        let (debit, credit) = if reverse {
            (l.ht_cents, 0)
        } else {
            (0, l.ht_cents)
        };
        lines.push(EntryLineInput {
            account_number: account,
            label: Some(l.label.clone()),
            debit_cents: debit,
            credit_cents: credit,
            vat_code: l.vat_code.clone(),
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        });
    }
    let vat = inv.vat_cents;
    let ttc = match inv.vat_mode.as_str() {
        "normale" => ht_sum + vat,
        _ => ht_sum, // sans / autoliquidation chantier
    };
    if inv.vat_mode == "normale" && vat > 0 {
        let (debit, credit) = if reverse { (vat, 0) } else { (0, vat) };
        lines.push(EntryLineInput {
            account_number: "44571".into(),
            label: Some(if reverse {
                "TVA collectée (avoir)".into()
            } else {
                "TVA collectée".into()
            }),
            debit_cents: debit,
            credit_cents: credit,
            vat_code: inv.vat_code.clone().or_else(|| {
                inv.lines.iter().find_map(|l| l.vat_code.clone())
            }),
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        });
    }
    // Client 411 : débit TTC en vente, crédit TTC en avoir
    let (debit, credit) = if reverse { (0, ttc) } else { (ttc, 0) };
    lines.push(EntryLineInput {
        account_number: "411".into(),
        label: Some(format!("Client {}", inv.third_party_code)),
        debit_cents: debit,
        credit_cents: credit,
        vat_code: None,
        third_party_id: Some(inv.third_party_id),
        comp_aux_type: Some("client".into()),
        comp_aux_num: Some(inv.third_party_code.clone()),
    });
    let _ = INVOICE_STATUSES;
    Ok(lines)
}

/// Texte PDF Helvetica / WinAnsi : accents FR + euro + séparateurs FR conservés.
fn ascii_safe(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '—' | '–' | '−' => '-',
            '•' => '·',
            '’' | '‘' | '‛' => '\'',
            '“' | '”' | '„' => '"',
            '…' => '.',
            '«' | '»' => '"',
            '\u{202F}' => '\u{00A0}', // espace fine → nbsp WinAnsi
            '·' | '\u{00A0}' | 'à' | 'â' | 'ä' | 'é' | 'è' | 'ê' | 'ë' | 'î' | 'ï' | 'ô' | 'ö'
            | 'ù' | 'û' | 'ü' | 'ç' | 'À' | 'Â' | 'Ä' | 'É' | 'È' | 'Ê' | 'Ë' | 'Î' | 'Ï' | 'Ô'
            | 'Ö' | 'Ù' | 'Û' | 'Ü' | 'Ç' | '€' => c,
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

fn cents_fr(cents: i64) -> String {
    crate::db::tax::format_cents_csv(cents)
}

/// Montant FR type référence 3PSI : `3 500,00 €`.
/// Espaces ASCII : Helvetica/printpdf ignore les nbsp WinAnsi dans `use_text`.
fn money_fr(cents: i64) -> String {
    let raw = cents_fr(cents);
    let (sign, body) = if let Some(rest) = raw.strip_prefix('-') {
        ("-", rest)
    } else {
        ("", raw.as_str())
    };
    let (int_part, dec) = body.split_once(',').unwrap_or((body, "00"));
    let grouped = group_thousands(int_part);
    format!("{sign}{grouped},{dec} €")
}

fn group_thousands(digits: &str) -> String {
    let chars: Vec<char> = digits.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(*c);
    }
    out
}

fn approx_width_mm(s: &str, size_pt: f32) -> f32 {
    let mut w_pt = 0.0_f32;
    for c in s.chars() {
        let factor = match c {
            'i' | 'l' | 'I' | 'j' | 't' | 'f' | 'r' | '.' | ',' | ' ' | '\u{00A0}' | '\'' => 0.28,
            'm' | 'w' | 'M' | 'W' => 0.83,
            '%' | '@' | '/' => 0.72,
            '€' | '·' => 0.55,
            _ => 0.55,
        };
        w_pt += factor * size_pt;
    }
    w_pt * 25.4 / 72.0
}

fn draw_text_right(
    layer: &PdfLayerReference,
    text: &str,
    size_pt: f32,
    right_x_mm: f32,
    y_mm: f32,
    font: &IndirectFontRef,
) {
    let safe = ascii_safe(text);
    let x = right_x_mm - approx_width_mm(&safe, size_pt);
    layer.use_text(safe, size_pt, Mm(x), Mm(y_mm), font);
}

fn spaced_siren(siren: &str) -> String {
    let digits: String = siren.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 9 {
        format!("{} {} {}", &digits[0..3], &digits[3..6], &digits[6..9])
    } else if siren.is_empty() {
        String::new()
    } else {
        siren.to_string()
    }
}

fn city_from_address(address: Option<&str>) -> Option<String> {
    let last = address?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .last()?;
    // « 75008 Paris, FR » / « 75011 Paris » → Paris
    let mut tokens = last
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .filter(|t| !t.chars().all(|c| c.is_ascii_digit()))
        .filter(|t| t.len() > 1 && t.to_ascii_uppercase() != "FR");
    tokens.next().map(|s| s.to_string())
}

fn commercial_footer_left(org: &Organization) -> String {
    let city = city_from_address(org.address.as_deref());
    let siren = spaced_siren(&org.siren);
    match city {
        Some(c) if !siren.is_empty() => {
            format!("{}, {} · {} {}", org.name, org.legal_form, c, siren)
        }
        Some(c) => format!("{}, {} · {}", org.name, org.legal_form, c),
        None if !siren.is_empty() => format!("{}, {} · {}", org.name, org.legal_form, siren),
        None => format!("{}, {}", org.name, org.legal_form),
    }
}

fn format_date_fr(iso: &str) -> String {
    // YYYY-MM-DD -> DD/MM/YYYY
    let b = iso.as_bytes();
    if b.len() >= 10 && b[4] == b'-' && b[7] == b'-' {
        format!("{}/{}/{}", &iso[8..10], &iso[5..7], &iso[0..4])
    } else {
        iso.to_string()
    }
}

fn qty_fr(quantity_milli: i64) -> String {
    if quantity_milli % 1000 == 0 {
        format!("{}", quantity_milli / 1000)
    } else {
        let whole = quantity_milli / 1000;
        let frac = (quantity_milli % 1000).abs();
        format!("{whole},{frac:03}").trim_end_matches('0').trim_end_matches(',').to_string()
    }
}

fn vat_pct_fr(bp: i32) -> String {
    if bp % 100 == 0 {
        format!("{} %", bp / 100)
    } else {
        format!("{},{:02} %", bp / 100, bp % 100)
    }
}

fn ink_color() -> Color {
    // Texte courant : anthracite lisible (pas le gris des bandeaux).
    Color::Rgb(Rgb::new(0.18, 0.18, 0.17, None))
}

/// Remplissage bandeaux (en-tête tableau, Total TTC, pied) — gris moyen, plus soft que noir.
fn header_fill() -> Color {
    Color::Rgb(Rgb::new(0.38, 0.38, 0.36, None)) // ≈ #61615C
}

fn rule_color() -> Color {
    Color::Rgb(Rgb::new(0.863, 0.863, 0.851, None))
}

/// Texte sur bandeau sombre (réf. Qonto/3PSI ≈ #F8F8F7, pas blanc pur).
fn band_text_color() -> Color {
    Color::Rgb(Rgb::new(0.973, 0.973, 0.969, None))
}

fn fill_rect(layer: &PdfLayerReference, x0: f32, y0: f32, x1: f32, y1: f32, color: Color) {
    layer.set_fill_color(color);
    let mut rect = Rect::new(Mm(x0), Mm(y0), Mm(x1), Mm(y1));
    rect = rect.with_mode(PaintMode::Fill);
    layer.add_rect(rect);
}

fn load_party(conn: &Connection, id: i64) -> DbResult<ThirdParty> {
    conn.query_row(
        "SELECT id, code, name, kind, account_number, active, siret, email, address, routing_id
         FROM third_party WHERE id = ?1",
        params![id],
        |r| {
            Ok(ThirdParty {
                id: r.get(0)?,
                code: r.get(1)?,
                name: r.get(2)?,
                kind: r.get(3)?,
                account_number: r.get(4)?,
                active: r.get::<_, i32>(5)? != 0,
                siret: r.get(6)?,
                email: r.get(7)?,
                address: r.get(8)?,
                routing_id: r.get(9)?,
            })
        },
    )
    .map_err(|_| "Tiers introuvable".into())
}

struct PdfLineView<'a> {
    label: &'a str,
    description: Option<&'a str>,
    quantity_milli: i64,
    unit_price_cents: i64,
    ht_cents: i64,
    vat_rate_bp: i32,
}

struct CommercialPdf<'a> {
    title: &'a str,
    number: &'a str,
    meta: Vec<(&'a str, String)>,
    org: &'a Organization,
    /// E-mail émetteur (optionnel — affiché sous l’adresse si présent).
    org_email: Option<&'a str>,
    client: &'a ThirdParty,
    lines: Vec<PdfLineView<'a>>,
    ht_cents: i64,
    vat_cents: i64,
    ttc_cents: i64,
    notes: Option<&'a str>,
    legal_mentions: Option<&'a str>,
    footer_left: String,
}

/// Layout calqué sur le modèle devis/facture 3PSI (A4 exact, métriques référence).
fn render_commercial_pdf(doc_title: &str, spec: &CommercialPdf<'_>, path: &Path) -> DbResult<()> {
    let (doc, page1, layer1) = PdfDocument::new(doc_title, Mm(210.0), Mm(PAGE_H_MM), "L1");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("PDF font: {e}"))?;
    let font_bold = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| format!("PDF font bold: {e}"))?;

    let margin_l = 12.7_f32;
    let margin_r = 11.6_f32;
    let page_w = 210.0_f32;
    let content_r = page_w - margin_r;
    // Colonnes tableau (mm) — bords droits calés sur la référence 3PSI
    let col_desc_r = 103.19_f32;
    let col_qty_right = 118.0_f32;
    let col_unit_right = 145.0_f32;
    let col_vat_right = 164.0_f32;
    let col_ht_right = 195.3_f32;
    let meta_value_x = 52.02_f32;
    let mid_x = 108.74_f32;
    let totals_label_x = 123.3_f32;
    let line_leading = 4.76_f32;
    // Pied identité (sans bandeau « Détails du paiement »)
    let footer_band_h = 23.28_f32;

    let mut layer = doc.get_page(page1).get_layer(layer1);
    let mut page_idx = 1usize;
    // Baseline titre : ~24 mm sous le bord haut (réf. glyph-top ≈ 17,6 mm)
    let mut y = 273.0_f32;

    let draw_logo = |layer: &PdfLayerReference, title_baseline: f32| {
        let Ok(decoder) =
            image_crate::codecs::png::PngDecoder::new(Cursor::new(COMMERCIAL_LOGO_PNG))
        else {
            return;
        };
        let Ok(image) = Image::try_from(decoder) else {
            return;
        };
        let px_w = image.image.width.0.max(1) as f32;
        // dpi tel que largeur image = COMMERCIAL_LOGO_MM.
        let dpi = px_w * 25.4 / COMMERCIAL_LOGO_MM;
        // Aligné sur le haut du titre (ascent Helvetica Bold 24 pt ≈ 6,1 mm).
        let logo_top = title_baseline + 6.1;
        let translate_x = content_r - COMMERCIAL_LOGO_MM;
        let translate_y = logo_top - COMMERCIAL_LOGO_MM;
        image.add_to_layer(
            layer.clone(),
            ImageTransform {
                translate_x: Some(Mm(translate_x)),
                translate_y: Some(Mm(translate_y)),
                dpi: Some(dpi),
                ..Default::default()
            },
        );
    };

    let draw_table_header = |layer: &PdfLayerReference, y_top: f32, header_h: f32| {
        let header_bot = y_top - header_h;
        // Cellules distinctes comme le modèle Qonto/3PSI
        let edges = [
            margin_l,
            col_desc_r,
            119.59_f32,
            146.58_f32,
            165.63_f32,
            content_r,
        ];
        for win in edges.windows(2) {
            fill_rect(layer, win[0], header_bot, win[1], y_top, header_fill());
        }
        layer.set_fill_color(band_text_color());
        let head_y = header_bot + 2.0;
        layer.use_text(
            "Description",
            7.5,
            Mm(margin_l + 3.18),
            Mm(head_y),
            &font_bold,
        );
        draw_text_right(layer, "Qté", 7.5, col_qty_right, head_y, &font_bold);
        draw_text_right(layer, "Prix unitaire", 7.5, col_unit_right, head_y, &font_bold);
        draw_text_right(layer, "TVA (%)", 7.5, col_vat_right, head_y, &font_bold);
        draw_text_right(layer, "Total HT", 7.5, col_ht_right, head_y, &font_bold);
    };

    let draw_footer = |layer: &PdfLayerReference, page_idx: usize, total_pages: &str| {
        fill_rect(layer, 0.0, 0.0, page_w, footer_band_h, header_fill());
        layer.set_fill_color(band_text_color());
        // Identité plus haute que le folio (réf. devis/facture).
        layer.use_text(
            ascii_safe(&spec.footer_left),
            7.5,
            Mm(margin_l),
            Mm(15.8),
            &font,
        );
        let right = format!("{} · {}", spec.number, total_pages);
        draw_text_right(layer, &right, 7.5, content_r, 9.5, &font);
        let _ = page_idx;
    };

    let start_page = |layer: &mut PdfLayerReference,
                      doc: &PdfDocumentReference,
                      page_idx: &mut usize,
                      y: &mut f32,
                      full_header: bool|
     -> DbResult<()> {
        if !full_header {
            let (p, l) = doc.add_page(Mm(210.0), Mm(PAGE_H_MM), "L");
            *layer = doc.get_page(p).get_layer(l);
            *page_idx += 1;
            *y = 280.0;
            layer.set_fill_color(ink_color());
            layer.use_text(
                ascii_safe(&format!("{} {}", spec.title, spec.number)),
                11.0,
                Mm(margin_l),
                Mm(*y),
                &font_bold,
            );
            *y -= 10.0;
        }
        Ok(())
    };

    // ——— En-tête titre + logo haut-droite + méta ———
    let title_baseline = y;
    draw_logo(&layer, title_baseline);
    layer.set_fill_color(ink_color());
    layer.use_text(ascii_safe(spec.title), 24.0, Mm(margin_l), Mm(y), &font_bold);
    y -= 16.2;

    for (label, value) in &spec.meta {
        if y < footer_band_h + 18.0 {
            draw_footer(&layer, page_idx, &format!("{page_idx}/?"));
            start_page(&mut layer, &doc, &mut page_idx, &mut y, false)?;
        }
        layer.set_fill_color(ink_color());
        layer.use_text(ascii_safe(label), 7.5, Mm(margin_l), Mm(y), &font_bold);
        layer.use_text(
            ascii_safe(value),
            7.5,
            Mm(meta_value_x),
            Mm(y),
            &font,
        );
        y -= 5.3;
    }

    y -= 10.0;

    // ——— Parties : émetteur (gauche) / client (droite) ———
    let party_top = y;
    layer.set_fill_color(ink_color());
    layer.use_text(ascii_safe(&spec.org.name), 9.0, Mm(margin_l), Mm(party_top), &font_bold);
    layer.use_text(
        ascii_safe(&spec.client.name),
        9.0,
        Mm(mid_x),
        Mm(party_top),
        &font_bold,
    );
    let mut y_left = party_top - 6.5;
    let mut y_right = party_top - 6.5;

    if let Some(addr) = &spec.org.address {
        for line in addr.lines() {
            layer.use_text(ascii_safe(line), 7.5, Mm(margin_l), Mm(y_left), &font);
            y_left -= line_leading;
        }
    }
    if let Some(addr) = &spec.client.address {
        for line in addr.lines() {
            layer.use_text(ascii_safe(line), 7.5, Mm(mid_x), Mm(y_right), &font);
            y_right -= line_leading;
        }
    }
    if let Some(email) = spec.org_email {
        layer.use_text(ascii_safe(email), 7.5, Mm(margin_l), Mm(y_left), &font);
        y_left -= line_leading;
    }
    if let Some(email) = &spec.client.email {
        layer.use_text(ascii_safe(email), 7.5, Mm(mid_x), Mm(y_right), &font);
        y_right -= line_leading;
    }
    let org_id = spec
        .org
        .siret
        .clone()
        .unwrap_or_else(|| spec.org.siren.clone());
    layer.use_text(ascii_safe(&org_id), 7.5, Mm(margin_l), Mm(y_left), &font);
    y_left -= line_leading;
    if let Some(siret) = &spec.client.siret {
        layer.use_text(ascii_safe(siret), 7.5, Mm(mid_x), Mm(y_right), &font);
        y_right -= line_leading;
    }

    y = y_left.min(y_right) - 12.0;

    // ——— En-tête tableau sombre ———
    let header_h = 6.35_f32;
    draw_table_header(&layer, y, header_h);
    y = y - header_h - 6.0;

    // ——— Lignes ———
    for line in &spec.lines {
        let desc_chunks = {
            let mut chunks = vec![line.label.to_string()];
            if let Some(desc) = line.description {
                for dline in desc.lines() {
                    chunks.extend(wrap_text(dline, 52));
                }
            }
            chunks
        };
        let block_h = (desc_chunks.len() as f32) * line_leading + 3.0;
        if y - block_h < footer_band_h + 12.0 {
            draw_footer(&layer, page_idx, &format!("{page_idx}/+"));
            start_page(&mut layer, &doc, &mut page_idx, &mut y, false)?;
            draw_table_header(&layer, y, header_h);
            y = y - header_h - 6.0;
        }

        let row_y = y;
        let last_y = row_y - ((desc_chunks.len().saturating_sub(1)) as f32) * line_leading;
        let amount_y = (row_y + last_y) / 2.0;

        layer.set_fill_color(ink_color());
        let mut dy = row_y;
        for chunk in &desc_chunks {
            layer.use_text(
                ascii_safe(chunk),
                7.5,
                Mm(margin_l + 3.0),
                Mm(dy),
                &font,
            );
            dy -= line_leading;
        }
        draw_text_right(
            &layer,
            &qty_fr(line.quantity_milli),
            7.5,
            col_qty_right,
            amount_y,
            &font,
        );
        draw_text_right(
            &layer,
            &money_fr(line.unit_price_cents),
            7.5,
            col_unit_right,
            amount_y,
            &font,
        );
        draw_text_right(
            &layer,
            &vat_pct_fr(line.vat_rate_bp),
            7.5,
            col_vat_right,
            amount_y,
            &font,
        );
        draw_text_right(
            &layer,
            &money_fr(line.ht_cents),
            7.5,
            col_ht_right,
            amount_y,
            &font,
        );
        y = dy - 2.0;
    }

    // Règle sous les lignes (segments colonnes)
    let rule_y = y + 1.5;
    let edges = [
        margin_l,
        col_desc_r,
        119.59_f32,
        146.58_f32,
        165.63_f32,
        content_r,
    ];
    for win in edges.windows(2) {
        fill_rect(&layer, win[0], rule_y, win[1], rule_y + 0.26, rule_color());
    }
    y -= 8.0;

    // ——— Totaux (à droite) ———
    layer.set_fill_color(ink_color());
    layer.use_text("Total HT", 7.5, Mm(totals_label_x), Mm(y), &font_bold);
    draw_text_right(
        &layer,
        &money_fr(spec.ht_cents),
        7.5,
        col_ht_right,
        y,
        &font,
    );
    y -= 8.47;
    layer.use_text(
        "Montant total de la TVA",
        7.5,
        Mm(totals_label_x),
        Mm(y),
        &font_bold,
    );
    draw_text_right(
        &layer,
        &money_fr(spec.vat_cents),
        7.5,
        col_ht_right,
        y,
        &font,
    );
    y -= 8.47;

    // Total TTC : deux cellules sombres adjacentes (réf. 3PSI), pas une barre unique.
    let ttc_h = 8.47_f32;
    let ttc_str = money_fr(spec.ttc_cents);
    let ttc_value_pad = 6.0_f32;
    let ttc_split = (content_r - approx_width_mm(&ttc_str, 7.5) - ttc_value_pad)
        .max(totals_label_x + 40.0)
        .min(content_r - 18.0);
    let ttc_bot = y - 2.0;
    fill_rect(
        &layer,
        totals_label_x - 3.18,
        ttc_bot,
        ttc_split,
        ttc_bot + ttc_h,
        header_fill(),
    );
    fill_rect(
        &layer,
        ttc_split,
        ttc_bot,
        content_r,
        ttc_bot + ttc_h,
        header_fill(),
    );
    layer.set_fill_color(band_text_color());
    layer.use_text("Total TTC", 7.5, Mm(totals_label_x), Mm(y), &font_bold);
    draw_text_right(
        &layer,
        &ttc_str,
        7.5,
        col_ht_right,
        y,
        &font_bold,
    );
    y -= 14.0;

    layer.set_fill_color(ink_color());
    if let Some(notes) = spec.notes {
        for chunk in wrap_text(notes, 95) {
            if y < footer_band_h + 8.0 {
                break;
            }
            layer.use_text(ascii_safe(&chunk), 7.5, Mm(margin_l), Mm(y), &font);
            y -= line_leading;
        }
        y -= 2.0;
    }
    if let Some(legal) = spec.legal_mentions {
        for chunk in wrap_text(legal, 95) {
            if y < footer_band_h + 8.0 {
                break;
            }
            layer.use_text(ascii_safe(&chunk), 7.5, Mm(margin_l), Mm(y), &font);
            y -= line_leading;
        }
    }

    let _ = y;
    // Pied de page identité uniquement (pas de bandeau paiement / RIB).
    draw_footer(
        &layer,
        page_idx,
        &format!("{page_idx}/{page_idx}"),
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    doc.save(&mut BufWriter::new(File::create(path)?))
        .map_err(|e| format!("PDF save: {e}"))?;
    Ok(())
}

pub fn export_quote_pdf(
    conn: &Connection,
    quote_id: i64,
    path: &Path,
) -> DbResult<()> {
    let quote = get_quote(conn, quote_id)?;
    let org = crate::db::load_organization(conn)?;
    let client = load_party(conn, quote.third_party_id)?;

    let mut meta = vec![
        ("Numéro de devis", quote.number.clone()),
        ("Date d'émission", format_date_fr(&quote.issue_date)),
    ];
    if let Some(vu) = &quote.valid_until {
        meta.push(("Date d'expiration", format_date_fr(vu)));
    }

    let lines: Vec<PdfLineView<'_>> = quote
        .lines
        .iter()
        .map(|l| PdfLineView {
            label: &l.label,
            description: l.description.as_deref(),
            quantity_milli: l.quantity_milli,
            unit_price_cents: l.unit_price_cents,
            ht_cents: l.ht_cents,
            vat_rate_bp: l.vat_rate_bp,
        })
        .collect();

    let footer_left = commercial_footer_left(&org);
    let org_email = org.email.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let spec = CommercialPdf {
        title: "Devis",
        number: &quote.number,
        meta,
        org: &org,
        org_email,
        client: &client,
        lines,
        ht_cents: quote.ht_cents,
        vat_cents: quote.vat_cents,
        ttc_cents: quote.ttc_cents,
        notes: quote.notes.as_deref(),
        legal_mentions: quote.legal_mentions.as_deref(),
        footer_left,
    };
    render_commercial_pdf("SasCompta Devis", &spec, path)
}

pub fn export_invoice_pdf(
    conn: &Connection,
    invoice_id: i64,
    path: &Path,
) -> DbResult<()> {
    let inv = get_invoice(conn, invoice_id)?;
    let org = crate::db::load_organization(conn)?;
    let client = load_party(conn, inv.third_party_id)?;
    let title = match inv.kind.as_str() {
        "deposit" => "Facture d'acompte",
        "balance" => "Facture de solde",
        "credit_note" => "Avoir",
        _ => "Facture",
    };
    let number = inv
        .number
        .clone()
        .unwrap_or_else(|| format!("BROUILLON-{}", inv.id));

    let mut meta = vec![
        ("Numéro de facture", number.clone()),
        ("Date d'émission", format_date_fr(&inv.issue_date)),
    ];
    if let Some(due) = &inv.due_date {
        meta.push(("Date d'échéance", format_date_fr(due)));
    }
    if let Some(po) = inv
        .purchase_order_number
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        meta.push(("Numéro de bon de commande", po.to_string()));
    }
    if let Some(qid) = inv.quote_id {
        if let Ok(q) = get_quote_header(conn, qid) {
            meta.push(("Numéro du devis associé", q.number));
        }
    }
    if let Some(parent) = inv.parent_invoice_id {
        meta.push(("Document lié", format!("#{parent}")));
    }

    let lines: Vec<PdfLineView<'_>> = inv
        .lines
        .iter()
        .map(|l| PdfLineView {
            label: &l.label,
            description: l.description.as_deref(),
            quantity_milli: l.quantity_milli,
            unit_price_cents: l.unit_price_cents,
            ht_cents: l.ht_cents,
            vat_rate_bp: l.vat_rate_bp,
        })
        .collect();

    let footer_left = commercial_footer_left(&org);
    let org_email = org.email.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let spec = CommercialPdf {
        title,
        number: &number,
        meta,
        org: &org,
        org_email,
        client: &client,
        lines,
        ht_cents: inv.ht_cents,
        vat_cents: inv.vat_cents,
        ttc_cents: inv.ttc_cents,
        notes: inv.notes.as_deref(),
        legal_mentions: inv.legal_mentions.as_deref(),
        footer_left,
    };
    render_commercial_pdf("SasCompta Facture", &spec, path)?;
    conn.execute(
        "UPDATE invoice SET pdf_path = ?1, updated_at = ?2 WHERE id = ?3",
        params![path.display().to_string(), now_iso(), invoice_id],
    )?;
    Ok(())
}

fn wrap_text(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if cur.is_empty() {
            cur = word.to_string();
        } else if cur.len() + 1 + word.len() <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            out.push(cur);
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn seed_demo_quotes(conn: &Connection) -> DbResult<()> {
    let client_id: i64 = conn.query_row(
        "SELECT id FROM third_party WHERE code = 'C001' LIMIT 1",
        [],
        |r| r.get(0),
    )?;
    // Coordonnées émetteur pour PDF (e-mail + bandeau paiement facture).
    let _ = conn.execute(
        "UPDATE organization SET
            email = COALESCE(NULLIF(TRIM(email), ''), 'contact@demo-sas.fr'),
            iban = COALESCE(NULLIF(TRIM(iban), ''), 'FR7616958000014398156414622'),
            bic = COALESCE(NULLIF(TRIM(bic), ''), 'QNTOFRP1XXX')
         WHERE id = 1",
        [],
    );
    // Adresse émetteur pour pied de page / bloc parties (aperçu PDF)
    let _ = conn.execute(
        "UPDATE organization SET address = '12 rue de la Démo\n75001 Paris, FR',
         siret = COALESCE(siret, '12345678200012')
         WHERE id = 1",
        [],
    );
    // enrich demo client
    let _ = conn.execute(
        "UPDATE third_party SET siret = '12345678200045', email = 'contact@gamma.fr',
         address = '8 avenue du Marche\n69001 Lyon, FR'
         WHERE id = ?1",
        params![client_id],
    );
    let input = CreateQuoteInput {
        third_party_id: client_id,
        issue_date: "2026-03-01".into(),
        valid_until: Some("2026-03-31".into()),
        vat_mode: "normale".into(),
        notes: Some("Installation détection incendie — atelier".into()),
        legal_mentions: None,
        lines: vec![
            QuoteLineInput {
                label: "Centrale détection addressable".into(),
                description: Some(
                    "Fourniture centrale + boucles adressables\nMise en service incluse".into(),
                ),
                quantity_milli: 1000,
                unit: "u".into(),
                unit_price_cents: 250_000,
                ht_cents: 0,
                vat_rate_bp: 2000,
                vat_code: Some("T20C".into()),
                account_number: "707".into(),
            },
            QuoteLineInput {
                label: "Main d'oeuvre pose".into(),
                description: Some("Pose et câblage sur site".into()),
                quantity_milli: 8000,
                unit: "h".into(),
                unit_price_cents: 6_500,
                ht_cents: 0,
                vat_rate_bp: 2000,
                vat_code: Some("T20C".into()),
                account_number: "706".into(),
            },
        ],
    };
    let q = create_quote(conn, &input)?;
    set_quote_status(conn, q.id, "sent")?;

    // Facture brouillon standalone (démo UI Factures)
    let inv_input = CreateInvoiceInput {
        kind: "standard".into(),
        third_party_id: client_id,
        quote_id: None,
        parent_invoice_id: None,
        issue_date: "2026-04-01".into(),
        due_date: Some("2026-04-30".into()),
        service_date: Some("2026-03-28".into()),
        vat_mode: "normale".into(),
        notes: Some("Maintenance annuelle détection".into()),
        legal_mentions: None,
        purchase_order_number: Some("BC-DEMO-2026".into()),
        lines: vec![InvoiceLineInput {
            label: "Contrat maintenance annuelle".into(),
            description: Some("Visites périodiques + rapport".into()),
            quantity_milli: 1000,
            unit: "u".into(),
            unit_price_cents: 120_000,
            ht_cents: 0,
            vat_rate_bp: 2000,
            vat_code: Some("T20C".into()),
            account_number: "706".into(),
            analytic_code: None,
        }],
    };
    let _ = create_invoice(conn, &inv_input)?;
    Ok(())
}


/* ——— Paiements + relances + lettrage soft ——— */

fn soft_lettre_for_invoice(inv: &Invoice) -> String {
    inv.number
        .as_ref()
        .map(|n| n.replace('-', ""))
        .unwrap_or_else(|| format!("FAC{}", inv.id))
}

fn apply_soft_lettre(conn: &Connection, entry_id: i64, lettre: &str, date: &str) -> DbResult<()> {
    // Lettrage soft mute des champs FEC (EcritureLettrage / DateLettrage) :
    // interdit sur exercice clôturé, comme toute autre mutation d'écriture.
    super::closing::assert_entry_fy_writable(conn, entry_id)?;
    conn.execute(
        "UPDATE entry SET ecriture_lettre = ?1, ecriture_date_lettre = ?2 WHERE id = ?3",
        params![lettre, date, entry_id],
    )?;
    Ok(())
}

fn refresh_invoice_payment_status(conn: &Connection, invoice_id: i64) -> DbResult<Invoice> {
    let inv = get_invoice(conn, invoice_id)?;
    let paid: i64 = conn.query_row(
        "SELECT COALESCE(SUM(amount_cents), 0) FROM payment WHERE invoice_id = ?1",
        params![invoice_id],
        |r| r.get(0),
    )?;
    let status = if paid <= 0 {
        // conserver issued/sent si aucun paiement
        if matches!(inv.status.as_str(), "partially_paid" | "paid") {
            "issued".to_string()
        } else {
            inv.status.clone()
        }
    } else if paid >= inv.ttc_cents {
        "paid".to_string()
    } else {
        "partially_paid".to_string()
    };
    conn.execute(
        "UPDATE invoice SET paid_cents = ?1, status = ?2, updated_at = ?3 WHERE id = ?4",
        params![paid, status, now_iso(), invoice_id],
    )?;
    get_invoice(conn, invoice_id)
}

pub fn list_payments(conn: &Connection, invoice_id: Option<i64>) -> DbResult<Vec<Payment>> {
    let mut sql = String::from(
        "SELECT p.id, p.invoice_id, i.number, tp.code, tp.name, p.paid_on, p.amount_cents,
                p.method, p.bank_ref, p.entry_id,
                (SELECT e.ecriture_lettre FROM entry e WHERE e.id = p.entry_id)
         FROM payment p
         JOIN invoice i ON i.id = p.invoice_id
         JOIN third_party tp ON tp.id = i.third_party_id",
    );
    if invoice_id.is_some() {
        sql.push_str(" WHERE p.invoice_id = ?1");
    }
    sql.push_str(" ORDER BY p.paid_on DESC, p.id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let map = |r: &rusqlite::Row<'_>| {
        Ok(Payment {
            id: r.get(0)?,
            invoice_id: r.get(1)?,
            invoice_number: r.get(2)?,
            third_party_code: r.get(3)?,
            third_party_name: r.get(4)?,
            paid_on: r.get(5)?,
            amount_cents: r.get(6)?,
            method: r.get(7)?,
            bank_ref: r.get(8)?,
            entry_id: r.get(9)?,
            lettre: r.get(10)?,
        })
    };
    let rows = if let Some(id) = invoice_id {
        stmt.query_map(params![id], map)?
    } else {
        stmt.query_map([], map)?
    };
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Enregistre un encaissement : paiement + écriture BQ **postée** (512/411) + lettrage soft.
///
/// La BQ est validée immédiatement pour aligner le statut commercial `paid` /
/// `partially_paid` avec le Grand livre / FEC (pas d'écart « payé mais brouillon banque »).
pub fn record_payment(conn: &Connection, input: &CreatePaymentInput) -> DbResult<Payment> {
    let inv = get_invoice(conn, input.invoice_id)?;
    if matches!(inv.status.as_str(), "draft" | "cancelled") {
        return Err("Paiement réservé aux factures émises".into());
    }
    if inv.kind == "credit_note" {
        return Err("Enregistrez plutôt un remboursement hors MVP paiement client".into());
    }
    if input.amount_cents <= 0 {
        return Err("Montant de paiement invalide".into());
    }
    let remaining = inv.ttc_cents - inv.paid_cents;
    if input.amount_cents > remaining {
        return Err(format!(
            "Montant trop élevé (reste dû {} centimes)",
            remaining
        )
        .into());
    }
    let method = if input.method.trim().is_empty() {
        "virement".to_string()
    } else {
        input.method.trim().to_string()
    };
    let piece = inv
        .number
        .clone()
        .unwrap_or_else(|| format!("FAC-{}", inv.id));
    let label = format!("Encaissement {piece}");
    let lines = vec![
        EntryLineInput {
            account_number: "512".into(),
            label: Some("Banque".into()),
            debit_cents: input.amount_cents,
            credit_cents: 0,
            vat_code: None,
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        },
        EntryLineInput {
            account_number: "411".into(),
            label: Some(format!("Client {}", inv.third_party_code)),
            debit_cents: 0,
            credit_cents: input.amount_cents,
            vat_code: None,
            third_party_id: Some(inv.third_party_id),
            comp_aux_type: Some("client".into()),
            comp_aux_num: Some(inv.third_party_code.clone()),
        },
    ];
    let entry_input = CreateEntryInput {
        journal_code: "BQ".into(),
        entry_date: input.paid_on.clone(),
        piece_ref: piece.clone(),
        label,
        lines,
    };

    let tx = conn.unchecked_transaction()?;
    let draft = ledger::create_draft(&tx, &entry_input)?;
    let entry = match ledger::post_entry(&tx, draft.id) {
        Ok(e) => e,
        Err(e) => {
            let _ = ledger::delete_draft(&tx, draft.id);
            return Err(e);
        }
    };
    let lettre = soft_lettre_for_invoice(&inv);
    apply_soft_lettre(&tx, entry.id, &lettre, &input.paid_on)?;
    // Lettrage soft sur l'écriture VTE liée si présente (même code) — erreur propagée.
    if let Some(vte_id) = inv.entry_id {
        apply_soft_lettre(&tx, vte_id, &lettre, &input.paid_on)?;
    }
    tx.execute(
        "INSERT INTO payment(invoice_id, paid_on, amount_cents, method, bank_ref, entry_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            input.invoice_id,
            input.paid_on,
            input.amount_cents,
            method,
            input.bank_ref,
            entry.id,
        ],
    )?;
    let payment_id = tx.last_insert_rowid();
    refresh_invoice_payment_status(&tx, input.invoice_id)?;
    tx.commit()?;
    list_payments(conn, Some(input.invoice_id))?
        .into_iter()
        .find(|p| p.id == payment_id)
        .ok_or_else(|| "Paiement introuvable après création".into())
}

fn days_between(from: &str, to: &str) -> i32 {
    // Dates ISO AAAA-MM-JJ — écart calendaire simple via chrono
    let parse = |s: &str| {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
    };
    match (parse(from), parse(to)) {
        (Some(a), Some(b)) => (b - a).num_days() as i32,
        _ => 0,
    }
}

fn today_ymd() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

fn reminder_body(inv: &Invoice, level: i32, days_overdue: i32, due: &str) -> String {
    let num = inv
        .number
        .clone()
        .unwrap_or_else(|| format!("#{}", inv.id));
    let due_cents = (inv.ttc_cents - inv.paid_cents).max(0);
    format!(
        "Relance J+{level} — Facture {num}\n\
         Client : {} ({})\n\
         Échéance : {due} (retard {days_overdue} j)\n\
         Reste dû : {} €\n\
         Merci de régulariser votre paiement.\n\
         — SasCompta (relance manuelle / PDF)",
        inv.third_party_name,
        inv.third_party_code,
        crate::db::tax::format_cents_csv(due_cents),
    )
}

pub fn list_reminders(conn: &Connection) -> DbResult<Vec<Reminder>> {
    let today = today_ymd();
    let mut stmt = conn.prepare(
        "SELECT r.id, r.invoice_id, i.number, tp.code, tp.name, r.level, i.due_date,
                i.ttc_cents, i.paid_cents, r.sent_at, r.channel, r.body_snapshot
         FROM reminder r
         JOIN invoice i ON i.id = r.invoice_id
         JOIN third_party tp ON tp.id = i.third_party_id
         ORDER BY r.level DESC, i.due_date ASC, r.id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        let due: Option<String> = r.get(6)?;
        let ttc: i64 = r.get(7)?;
        let paid: i64 = r.get(8)?;
        let days = due
            .as_deref()
            .map(|d| days_between(d, &today).max(0))
            .unwrap_or(0);
        Ok(Reminder {
            id: r.get(0)?,
            invoice_id: r.get(1)?,
            invoice_number: r.get(2)?,
            third_party_code: r.get(3)?,
            third_party_name: r.get(4)?,
            level: r.get(5)?,
            due_date: due,
            days_overdue: days,
            amount_due_cents: (ttc - paid).max(0),
            sent_at: r.get(9)?,
            channel: r.get(10)?,
            body_snapshot: r.get(11)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Crée les relances J+7 / J+15 / J+30 manquantes pour les factures échues non soldées.
pub fn generate_due_reminders(conn: &Connection) -> DbResult<Vec<Reminder>> {
    let today = today_ymd();
    let invoices = list_invoices(conn)?;
    let mut created_ids = Vec::new();
    for inv in invoices {
        if !matches!(
            inv.status.as_str(),
            "issued" | "sent" | "partially_paid"
        ) {
            continue;
        }
        if inv.paid_cents >= inv.ttc_cents {
            continue;
        }
        let Some(due) = inv.due_date.clone() else {
            continue;
        };
        let overdue = days_between(&due, &today);
        if overdue < 7 {
            continue;
        }
        for level in [7_i32, 15, 30] {
            if overdue < level {
                continue;
            }
            let exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM reminder WHERE invoice_id = ?1 AND level = ?2",
                params![inv.id, level],
                |r| r.get(0),
            )?;
            if exists > 0 {
                continue;
            }
            let body = reminder_body(&inv, level, overdue, &due);
            conn.execute(
                "INSERT INTO reminder(invoice_id, level, sent_at, channel, body_snapshot)
                 VALUES (?1, ?2, NULL, 'pdf', ?3)",
                params![inv.id, level, body],
            )?;
            created_ids.push(conn.last_insert_rowid());
        }
    }
    let all = list_reminders(conn)?;
    Ok(all
        .into_iter()
        .filter(|r| created_ids.contains(&r.id))
        .collect())
}

pub fn mark_reminder_sent(conn: &Connection, id: i64) -> DbResult<Reminder> {
    let n = conn.execute(
        "UPDATE reminder SET sent_at = ?1 WHERE id = ?2 AND sent_at IS NULL",
        params![now_iso(), id],
    )?;
    if n == 0 {
        // déjà envoyée ou introuvable
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM reminder WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err("Relance introuvable".into());
        }
    }
    list_reminders(conn)?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| "Relance introuvable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_demo_workspace, open_connection};
    use tempfile::tempdir;

    #[test]
    fn line_ht_and_vat() {
        assert_eq!(compute_line_ht(1000, 2500), 2500);
        assert_eq!(compute_line_ht(2500, 1000), 2500); // 2.5 × 10 €
        assert_eq!(compute_vat_cents(10_000, 2000), 2000);
    }

    #[test]
    fn quote_crud_pdf_and_convert_hooks() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bill.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        let version: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, crate::db::SCHEMA_VERSION.to_string());

        let quotes = list_quotes(&conn).unwrap();
        assert!(!quotes.is_empty());
        let q = &quotes[0];
        assert_eq!(q.status, "sent");
        assert!(q.ttc_cents > q.ht_cents);

        let accepted = set_quote_status(&conn, q.id, "accepted").unwrap();
        assert_eq!(accepted.status, "accepted");

        let inv = convert_quote_to_invoice(&conn, q.id).unwrap();
        assert_eq!(inv.status, "draft");
        assert!(inv.number.is_none()); // numéro à l'émission
        assert_eq!(inv.quote_id, Some(q.id));
        assert!(
            inv.due_date.is_some(),
            "conversion devis→facture doit poser une échéance (relances)"
        );
        let again = convert_quote_to_invoice(&conn, q.id).unwrap();
        assert_eq!(again.id, inv.id); // idempotent

        let entry = build_invoice_entry_draft(&conn, inv.id).unwrap();
        assert_eq!(entry.status, "draft");
        assert_eq!(entry.journal_code, "VTE");
        let entry2 = build_invoice_entry_draft(&conn, inv.id).unwrap();
        assert_eq!(entry2.id, entry.id); // même draft, lignes remplacées
        let linked: i64 = conn
            .query_row(
                "SELECT entry_id FROM invoice WHERE id = ?1",
                params![inv.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(linked, entry.id);

        let pdf = dir.path().join("devis.pdf");
        export_quote_pdf(&conn, q.id, &pdf).unwrap();
        assert!(pdf.exists());
        assert!(pdf.metadata().unwrap().len() > 100);
        // Helpers calés sur la référence 3PSI
        assert_eq!(money_fr(350_000), "3 500,00 €");
        assert_eq!(spaced_siren("933627960"), "933 627 960");
        assert_eq!(
            city_from_address(Some("60 rue François 1er\n75008 Paris, FR")).as_deref(),
            Some("Paris")
        );
        assert!(
            commercial_footer_left(&crate::db::load_organization(&conn).unwrap())
                .contains('·'),
            "pied de page avec point médian"
        );
    }

    #[test]
    fn cannot_delete_non_draft() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bill2.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        let q = list_quotes(&conn).unwrap().into_iter().next().unwrap();
        assert!(delete_quote(&conn, q.id).is_err());
    }

    #[test]
    fn issue_numbering_credit_note_and_deposit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bill3.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        let draft = list_invoices(&conn)
            .unwrap()
            .into_iter()
            .find(|i| i.status == "draft")
            .expect("demo invoice");
        assert!(draft.number.is_none());

        let issued = issue_invoice(&conn, draft.id).unwrap();
        assert_eq!(issued.status, "issued");
        let num = issued.number.as_deref().unwrap();
        // Date démo facture = 2026-04-01 → F2026-04NN
        assert!(num.starts_with("F2026-04"), "{num}");
        assert!(issued.entry_id.is_some()); // VTE draft auto

        let entry = get_invoice(&conn, issued.id).unwrap();
        let entry_id = entry.entry_id.unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM entry WHERE id = ?1",
                params![entry_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "draft");

        // Regenerate VTE = same draft id
        let rebuilt = build_invoice_entry_draft(&conn, issued.id).unwrap();
        assert_eq!(rebuilt.id, entry_id);

        let credit = create_credit_note(&conn, issued.id).unwrap();
        assert_eq!(credit.kind, "credit_note");
        assert_eq!(credit.status, "draft");
        assert_eq!(credit.parent_invoice_id, Some(issued.id));
        let credit_issued = issue_invoice(&conn, credit.id).unwrap();
        let credit_num = credit_issued.number.as_deref().unwrap();
        assert!(
            credit_num.starts_with("F2026-"),
            "avoir annuel FAAAA-MMNN, got {credit_num}"
        );
        assert_ne!(credit_num, num, "avoir ≠ facture d’origine");

        let deposit = create_deposit_invoice(
            &conn,
            issued.id,
            50_000,
            2000,
            Some("T20C".into()),
        )
        .unwrap();
        assert_eq!(deposit.kind, "deposit");
        assert_eq!(deposit.lines[0].account_number, "4191");
        let dep_issued = issue_invoice(&conn, deposit.id).unwrap();
        let dep_num = dep_issued.number.as_deref().unwrap();
        assert!(
            dep_num.starts_with("F2026-"),
            "acompte annuel FAAAA-MMNN, got {dep_num}"
        );
        assert_ne!(dep_num, num);
        assert_ne!(dep_num, credit_num);

        let pdf = dir.path().join("facture.pdf");
        export_invoice_pdf(&conn, issued.id, &pdf).unwrap();
        assert!(pdf.exists());

        // Pas de double post silencieux
        ledger::post_entry(&conn, entry_id).unwrap();
        assert!(build_invoice_entry_draft(&conn, issued.id).is_err());
    }

    #[test]
    fn payment_bq_soft_lettre_and_reminders() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pay.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        let draft = list_invoices(&conn)
            .unwrap()
            .into_iter()
            .find(|i| i.status == "draft")
            .unwrap();
        // échéance passée pour relances
        conn.execute(
            "UPDATE invoice SET due_date = '2026-01-01' WHERE id = ?1",
            params![draft.id],
        )
        .unwrap();
        let issued = issue_invoice(&conn, draft.id).unwrap();
        let vte = issued.entry_id.unwrap();
        ledger::post_entry(&conn, vte).unwrap();

        let pay = record_payment(
            &conn,
            &CreatePaymentInput {
                invoice_id: issued.id,
                paid_on: "2026-04-10".into(),
                amount_cents: issued.ttc_cents / 2,
                method: "virement".into(),
                bank_ref: Some("VIR-1".into()),
            },
        )
        .unwrap();
        assert!(pay.entry_id.is_some());
        let expected_lettre = issued.number.as_ref().unwrap().replace('-', "");
        assert_eq!(pay.lettre.as_deref(), Some(expected_lettre.as_str()));
        let partial = get_invoice(&conn, issued.id).unwrap();
        assert_eq!(partial.status, "partially_paid");

        // même lettre soft sur VTE
        let vte_lettre: String = conn
            .query_row(
                "SELECT ecriture_lettre FROM entry WHERE id = ?1",
                params![vte],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(vte_lettre, pay.lettre.unwrap());

        let bq_id = pay.entry_id.unwrap();
        let bq = ledger::get_entry(&conn, bq_id).unwrap();
        assert_eq!(bq.journal_code, "BQ");
        assert_eq!(
            bq.status, "posted",
            "encaissement doit poster la BQ pour aligner paid et Grand livre"
        );
        assert!(bq.ecriture_num.as_deref().unwrap_or("").starts_with("BQ-"));

        let rest = get_invoice(&conn, issued.id).unwrap();
        let _ = record_payment(
            &conn,
            &CreatePaymentInput {
                invoice_id: issued.id,
                paid_on: "2026-04-15".into(),
                amount_cents: rest.ttc_cents - rest.paid_cents,
                method: "cheque".into(),
                bank_ref: None,
            },
        )
        .unwrap();
        assert_eq!(get_invoice(&conn, issued.id).unwrap().status, "paid");

        // Relances : facture soldée → pas de nouvelle ; créer une autre échue
        // (dates dans l'exercice ouvert 2026 — la VTE refuse une date hors exercice)
        let other = create_invoice(
            &conn,
            &CreateInvoiceInput {
                kind: "standard".into(),
                third_party_id: issued.third_party_id,
                quote_id: None,
                parent_invoice_id: None,
                issue_date: "2026-01-05".into(),
                due_date: Some("2026-01-10".into()),
                service_date: None,
                vat_mode: "normale".into(),
                notes: None,
                legal_mentions: None,
                purchase_order_number: None,
                lines: vec![InvoiceLineInput {
                    label: "Prestation".into(),
                    description: None,
                    quantity_milli: 1000,
                    unit: "u".into(),
                    unit_price_cents: 10_000,
                    ht_cents: 10_000,
                    vat_rate_bp: 2000,
                    vat_code: Some("T20C".into()),
                    account_number: "706".into(),
                    analytic_code: None,
                }],
            },
        )
        .unwrap();
        let other = issue_invoice(&conn, other.id).unwrap();
        let created = generate_due_reminders(&conn).unwrap();
        assert!(!created.is_empty());
        assert!(created.iter().any(|r| r.invoice_id == other.id && r.level == 7));
        assert!(created.iter().any(|r| r.level == 30));
        let again = generate_due_reminders(&conn).unwrap();
        assert!(again.is_empty()); // idempotent
        let first = list_reminders(&conn)
            .unwrap()
            .into_iter()
            .find(|r| r.invoice_id == other.id && r.level == 7)
            .unwrap();
        let marked = mark_reminder_sent(&conn, first.id).unwrap();
        assert!(marked.sent_at.is_some());
    }

    #[test]
    fn configure_next_invoice_numbers_and_allocate() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("seq.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        let listed = list_document_sequences(&conn).unwrap();
        assert_eq!(listed.len(), DOCUMENT_SEQUENCE_KINDS.len());
        let fact = listed.iter().find(|s| s.kind == "FACT").unwrap();
        assert_eq!(fact.next_num, 1);
        assert!(
            fact.preview.starts_with("F2026-") && fact.preview.ends_with("01"),
            "{}",
            fact.preview
        );

        // Continuer une série externe : prochain = 42 → F2026-MM42
        let updated = set_document_sequences(
            &conn,
            &[
                SetDocumentSequenceInput {
                    kind: "FACT".into(),
                    next_num: 42,
                },
                SetDocumentSequenceInput {
                    kind: "DEV".into(),
                    next_num: 7,
                },
            ],
        )
        .unwrap();
        let fact_preview = &updated.iter().find(|s| s.kind == "FACT").unwrap().preview;
        assert!(fact_preview.ends_with("42"), "{fact_preview}");
        assert!(fact_preview.starts_with("F2026-"), "{fact_preview}");
        let dev_preview = &updated.iter().find(|s| s.kind == "DEV").unwrap().preview;
        assert!(dev_preview.starts_with("D2026-"), "{dev_preview}");
        assert!(dev_preview.ends_with("07"), "{dev_preview}");

        let draft = list_invoices(&conn)
            .unwrap()
            .into_iter()
            .find(|i| i.status == "draft")
            .expect("demo invoice");
        let issued = issue_invoice(&conn, draft.id).unwrap();
        // issue_date démo = 2026-04-01
        assert_eq!(issued.number.as_deref(), Some("F2026-0442"));

        let after = list_document_sequences(&conn).unwrap();
        let fact_after = after.iter().find(|s| s.kind == "FACT").unwrap();
        assert_eq!(fact_after.next_num, 43);
        assert_eq!(fact_after.min_next_num, 43);

        // Impossible de redescendre sous un numéro déjà émis
        let err = set_document_sequences(
            &conn,
            &[SetDocumentSequenceInput {
                kind: "FACT".into(),
                next_num: 42,
            }],
        )
        .unwrap_err();
        assert!(err.to_string().contains("≥ 43"), "{err}");

        let deposit = create_deposit_invoice(&conn, issued.id, 10_000, 2000, None).unwrap();
        let dep_issued = issue_invoice(&conn, deposit.id).unwrap();
        assert_eq!(dep_issued.number.as_deref(), Some("F2026-0443"));

        let credit = create_credit_note(&conn, issued.id).unwrap();
        let credit_issued = issue_invoice(&conn, credit.id).unwrap();
        assert_eq!(credit_issued.number.as_deref(), Some("F2026-0444"));
    }

    #[test]
    fn annual_sequence_format_and_year_rollover() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rollover.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        let client_id: i64 = conn
            .query_row(
                "SELECT id FROM third_party WHERE code = 'C001' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();

        let line = InvoiceLineInput {
            label: "Prestation".into(),
            description: None,
            quantity_milli: 1000,
            unit: "u".into(),
            unit_price_cents: 10_000,
            ht_cents: 10_000,
            vat_rate_bp: 2000,
            vat_code: Some("T20C".into()),
            account_number: "706".into(),
            analytic_code: None,
        };

        let nov = create_invoice(
            &conn,
            &CreateInvoiceInput {
                kind: "standard".into(),
                third_party_id: client_id,
                quote_id: None,
                parent_invoice_id: None,
                issue_date: "2026-11-15".into(),
                due_date: None,
                service_date: None,
                vat_mode: "normale".into(),
                notes: None,
                legal_mentions: None,
                purchase_order_number: None,
                lines: vec![line.clone()],
            },
        )
        .unwrap();
        let nov_issued = issue_invoice(&conn, nov.id).unwrap();
        assert_eq!(nov_issued.number.as_deref(), Some("F2026-1101"));

        let dec = create_invoice(
            &conn,
            &CreateInvoiceInput {
                kind: "standard".into(),
                third_party_id: client_id,
                quote_id: None,
                parent_invoice_id: None,
                issue_date: "2026-12-20".into(),
                due_date: None,
                service_date: None,
                vat_mode: "normale".into(),
                notes: None,
                legal_mentions: None,
                purchase_order_number: None,
                lines: vec![line.clone()],
            },
        )
        .unwrap();
        let dec_issued = issue_invoice(&conn, dec.id).unwrap();
        // Même année civile → compteur continue, mois = décembre
        assert_eq!(dec_issued.number.as_deref(), Some("F2026-1202"));

        // Rollover calendaire 2027 : allocation seule (l'émission complète
        // exige un exercice ouvert couvrant la date — testé ailleurs).
        let n27 = allocate_document_number(&conn, "FACT", "2027-01-05").unwrap();
        assert_eq!(n27, "F2027-0101");

        // Devis : série séparée, préfixe D, même format annuel
        let q = create_quote(
            &conn,
            &CreateQuoteInput {
                third_party_id: client_id,
                issue_date: "2026-11-01".into(),
                valid_until: None,
                vat_mode: "normale".into(),
                notes: None,
                legal_mentions: None,
                lines: vec![QuoteLineInput {
                    label: "Étude".into(),
                    description: None,
                    quantity_milli: 1000,
                    unit: "u".into(),
                    unit_price_cents: 5_000,
                    ht_cents: 0,
                    vat_rate_bp: 2000,
                    vat_code: Some("T20C".into()),
                    account_number: "706".into(),
                }],
            },
        )
        .unwrap();
        // Seed démo a déjà un devis D2026-0301 → prochain = 02
        assert_eq!(q.number, "D2026-1102");

        let q27 = create_quote(
            &conn,
            &CreateQuoteInput {
                third_party_id: client_id,
                issue_date: "2027-02-10".into(),
                valid_until: None,
                vat_mode: "normale".into(),
                notes: None,
                legal_mentions: None,
                lines: vec![QuoteLineInput {
                    label: "Étude 2027".into(),
                    description: None,
                    quantity_milli: 1000,
                    unit: "u".into(),
                    unit_price_cents: 5_000,
                    ht_cents: 0,
                    vat_rate_bp: 2000,
                    vat_code: Some("T20C".into()),
                    account_number: "706".into(),
                }],
            },
        )
        .unwrap();
        assert_eq!(q27.number, "D2027-0201");
    }

    #[test]
    fn preserves_legacy_prefixed_numbers_when_continuing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy-seq.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        // Simule une facture déjà émise hors nouveau format
        conn.execute(
            "UPDATE invoice SET number = 'FACT-2026-0041', status = 'issued'
             WHERE id = (SELECT id FROM invoice WHERE status = 'draft' LIMIT 1)",
            [],
        )
        .unwrap();

        let listed = list_document_sequences(&conn).unwrap();
        let fact = listed.iter().find(|s| s.kind == "FACT").unwrap();
        assert_eq!(fact.min_next_num, 42);

        let client_id: i64 = conn
            .query_row(
                "SELECT id FROM third_party WHERE code = 'C001' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let draft = create_invoice(
            &conn,
            &CreateInvoiceInput {
                kind: "standard".into(),
                third_party_id: client_id,
                quote_id: None,
                parent_invoice_id: None,
                issue_date: "2026-11-01".into(),
                due_date: None,
                service_date: None,
                vat_mode: "normale".into(),
                notes: None,
                legal_mentions: None,
                purchase_order_number: None,
                lines: vec![InvoiceLineInput {
                    label: "Suite".into(),
                    description: None,
                    quantity_milli: 1000,
                    unit: "u".into(),
                    unit_price_cents: 1_000,
                    ht_cents: 1_000,
                    vat_rate_bp: 2000,
                    vat_code: Some("T20C".into()),
                    account_number: "706".into(),
                    analytic_code: None,
                }],
            },
        )
        .unwrap();
        let issued = issue_invoice(&conn, draft.id).unwrap();
        assert_eq!(issued.number.as_deref(), Some("F2026-1142"));
    }

    #[test]
    fn preserves_dashed_aaaa_mm_nnnn_when_continuing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("dashed-seq.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        // Simule une facture déjà émise au format 0.6.8
        conn.execute(
            "UPDATE invoice SET number = '2026-10-0041', status = 'issued'
             WHERE id = (SELECT id FROM invoice WHERE status = 'draft' LIMIT 1)",
            [],
        )
        .unwrap();

        let listed = list_document_sequences(&conn).unwrap();
        let fact = listed.iter().find(|s| s.kind == "FACT").unwrap();
        assert_eq!(fact.min_next_num, 42);

        let client_id: i64 = conn
            .query_row(
                "SELECT id FROM third_party WHERE code = 'C001' LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let draft = create_invoice(
            &conn,
            &CreateInvoiceInput {
                kind: "standard".into(),
                third_party_id: client_id,
                quote_id: None,
                parent_invoice_id: None,
                issue_date: "2026-11-01".into(),
                due_date: None,
                service_date: None,
                vat_mode: "normale".into(),
                notes: None,
                legal_mentions: None,
                purchase_order_number: None,
                lines: vec![InvoiceLineInput {
                    label: "Suite".into(),
                    description: None,
                    quantity_milli: 1000,
                    unit: "u".into(),
                    unit_price_cents: 1_000,
                    ht_cents: 1_000,
                    vat_rate_bp: 2000,
                    vat_code: Some("T20C".into()),
                    account_number: "706".into(),
                    analytic_code: None,
                }],
            },
        )
        .unwrap();
        let issued = issue_invoice(&conn, draft.id).unwrap();
        assert_eq!(issued.number.as_deref(), Some("F2026-1142"));
        // L’ancien numéro reste tel quel
        let old: String = conn
            .query_row(
                "SELECT number FROM invoice WHERE number = '2026-10-0041'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old, "2026-10-0041");
    }
}
