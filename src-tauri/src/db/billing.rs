//! Module commercial — devis, factures (schéma), PDF, hooks d'écriture VTE.

use super::ledger;
use super::DbResult;
use crate::models::*;
use chrono::Utc;
use printpdf::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub const SCHEMA_BILLING_VERSION: i32 = 9;

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
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS invoice_line (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            invoice_id INTEGER NOT NULL REFERENCES invoice(id) ON DELETE CASCADE,
            line_no INTEGER NOT NULL,
            label TEXT NOT NULL,
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
        "#,
    )?;
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

fn fy_label(conn: &Connection, fy_id: i64) -> DbResult<String> {
    conn.query_row(
        "SELECT label FROM fiscal_year WHERE id = ?1",
        params![fy_id],
        |r| r.get(0),
    )
    .map_err(|e| e.into())
}

/// Réserve atomiquement le prochain numéro (DEV-2026-0001…).
pub fn allocate_document_number(
    conn: &Connection,
    kind: &str,
    fiscal_year_id: i64,
) -> DbResult<String> {
    let next: i64 = conn
        .query_row(
            "SELECT next_num FROM document_sequence WHERE kind = ?1 AND fiscal_year_id = ?2",
            params![kind, fiscal_year_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(1);
    if next == 1 {
        conn.execute(
            "INSERT INTO document_sequence(kind, fiscal_year_id, next_num)
             VALUES (?1, ?2, 2)
             ON CONFLICT(kind, fiscal_year_id) DO UPDATE SET next_num = excluded.next_num",
            params![kind, fiscal_year_id],
        )?;
    } else {
        conn.execute(
            "UPDATE document_sequence SET next_num = next_num + 1
             WHERE kind = ?1 AND fiscal_year_id = ?2",
            params![kind, fiscal_year_id],
        )?;
    }
    let label = fy_label(conn, fiscal_year_id)?;
    Ok(format!("{kind}-{label}-{next:04}"))
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
        "SELECT id, quote_id, line_no, label, quantity_milli, unit, unit_price_cents,
                ht_cents, vat_rate_bp, vat_code, account_number
         FROM quote_line WHERE quote_id = ?1 ORDER BY line_no",
    )?;
    let rows = stmt.query_map(params![quote_id], |r| {
        Ok(QuoteLine {
            id: r.get(0)?,
            quote_id: r.get(1)?,
            line_no: r.get(2)?,
            label: r.get(3)?,
            quantity_milli: r.get(4)?,
            unit: r.get(5)?,
            unit_price_cents: r.get(6)?,
            ht_cents: r.get(7)?,
            vat_rate_bp: r.get(8)?,
            vat_code: r.get(9)?,
            account_number: r.get(10)?,
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

fn insert_quote_lines(
    conn: &Connection,
    quote_id: i64,
    lines: &[(QuoteLineInput, i64)],
) -> DbResult<()> {
    for (i, (line, _)) in lines.iter().enumerate() {
        let label = line.label.trim();
        if label.is_empty() {
            return Err("Libellé de ligne requis".into());
        }
        if line.quantity_milli <= 0 {
            return Err("Quantité invalide".into());
        }
        let account = if line.account_number.trim().is_empty() {
            "706"
        } else {
            line.account_number.trim()
        };
        conn.execute(
            "INSERT INTO quote_line(
                quote_id, line_no, label, quantity_milli, unit, unit_price_cents,
                ht_cents, vat_rate_bp, vat_code, account_number
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                quote_id,
                (i + 1) as i32,
                label,
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
    let fy_id = open_fy_id(conn)?;
    let number = allocate_document_number(conn, "DEV", fy_id)?;
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
        created_at: r.get(25)?,
        updated_at: r.get(26)?,
        lines: Vec::new(),
    })
}

fn load_invoice_lines(conn: &Connection, invoice_id: i64) -> DbResult<Vec<InvoiceLine>> {
    let mut stmt = conn.prepare(
        "SELECT id, invoice_id, line_no, label, quantity_milli, unit, unit_price_cents,
                ht_cents, vat_rate_bp, vat_code, account_number, analytic_code
         FROM invoice_line WHERE invoice_id = ?1 ORDER BY line_no",
    )?;
    let rows = stmt.query_map(params![invoice_id], |r| {
        Ok(InvoiceLine {
            id: r.get(0)?,
            invoice_id: r.get(1)?,
            line_no: r.get(2)?,
            label: r.get(3)?,
            quantity_milli: r.get(4)?,
            unit: r.get(5)?,
            unit_price_cents: r.get(6)?,
            ht_cents: r.get(7)?,
            vat_rate_bp: r.get(8)?,
            vat_code: r.get(9)?,
            account_number: r.get(10)?,
            analytic_code: r.get(11)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_invoice(conn: &Connection, id: i64) -> DbResult<Invoice> {
    let mut inv = conn
        .query_row(
            "SELECT i.id, i.number, i.kind, i.third_party_id, tp.code, tp.name,
                    i.quote_id, i.parent_invoice_id, i.status, i.issue_date, i.due_date,
                    i.service_date, i.ht_cents, i.vat_cents, i.ttc_cents, i.paid_cents,
                    i.vat_mode, i.vat_code, i.einvoice_profile, i.einvoice_status,
                    i.entry_id, i.pdf_path, i.facturx_path, i.notes, i.legal_mentions,
                    i.created_at, i.updated_at
             FROM invoice i
             JOIN third_party tp ON tp.id = i.third_party_id
             WHERE i.id = ?1",
            params![id],
            map_invoice_row,
        )
        .map_err(|_| "Facture introuvable")?;
    inv.lines = load_invoice_lines(conn, id)?;
    Ok(inv)
}

pub fn list_invoices(conn: &Connection) -> DbResult<Vec<Invoice>> {
    let mut stmt = conn.prepare(
        "SELECT i.id, i.number, i.kind, i.third_party_id, tp.code, tp.name,
                i.quote_id, i.parent_invoice_id, i.status, i.issue_date, i.due_date,
                i.service_date, i.ht_cents, i.vat_cents, i.ttc_cents, i.paid_cents,
                i.vat_mode, i.vat_code, i.einvoice_profile, i.einvoice_status,
                i.entry_id, i.pdf_path, i.facturx_path, i.notes, i.legal_mentions,
                i.created_at, i.updated_at
         FROM invoice i
         JOIN third_party tp ON tp.id = i.third_party_id
         ORDER BY i.issue_date DESC, i.id DESC",
    )?;
    let rows = stmt.query_map([], map_invoice_row)?;
    let mut out = Vec::new();
    for row in rows {
        let mut inv = row?;
        inv.lines = load_invoice_lines(conn, inv.id)?;
        out.push(inv);
    }
    Ok(out)
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
    conn.execute(
        "INSERT INTO invoice(
            number, kind, third_party_id, quote_id, status, issue_date, due_date,
            ht_cents, vat_cents, ttc_cents, paid_cents, vat_mode, notes, legal_mentions,
            created_at, updated_at
         ) VALUES (NULL, 'standard', ?1, ?2, 'draft', ?3, NULL, ?4, ?5, ?6, 0, ?7, ?8, ?9, ?10, ?10)",
        params![
            quote.third_party_id,
            quote_id,
            quote.issue_date,
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
                invoice_id, line_no, label, quantity_milli, unit, unit_price_cents,
                ht_cents, vat_rate_bp, vat_code, account_number
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                invoice_id,
                line.line_no,
                line.label,
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
    let mut lines = Vec::new();
    let mut ht_sum = 0i64;
    for l in &inv.lines {
        ht_sum += l.ht_cents;
        lines.push(EntryLineInput {
            account_number: l.account_number.clone(),
            label: Some(l.label.clone()),
            debit_cents: 0,
            credit_cents: l.ht_cents,
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
        lines.push(EntryLineInput {
            account_number: "44571".into(),
            label: Some("TVA collectée".into()),
            debit_cents: 0,
            credit_cents: vat,
            vat_code: inv.vat_code.clone().or_else(|| {
                inv.lines
                    .iter()
                    .find_map(|l| l.vat_code.clone())
            }),
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        });
    }
    lines.push(EntryLineInput {
        account_number: "411".into(),
        label: Some(format!("Client {}", inv.third_party_code)),
        debit_cents: ttc,
        credit_cents: 0,
        vat_code: None,
        third_party_id: Some(inv.third_party_id),
        comp_aux_type: Some("client".into()),
        comp_aux_num: Some(inv.third_party_code.clone()),
    });
    let _ = INVOICE_STATUSES; // keep statuses referenced for future issue flow
    Ok(lines)
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
            '€' => 'E',
            'À' | 'Â' | 'Ä' => 'A',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'Î' | 'Ï' => 'I',
            'Ô' | 'Ö' => 'O',
            'Ù' | 'Û' | 'Ü' => 'U',
            'Ç' => 'C',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

fn cents_fr(cents: i64) -> String {
    crate::db::tax::format_cents_csv(cents)
}

pub fn export_quote_pdf(
    conn: &Connection,
    quote_id: i64,
    path: &Path,
) -> DbResult<()> {
    let quote = get_quote(conn, quote_id)?;
    let org = crate::db::load_organization(conn)?;
    let (doc, page1, layer1) = PdfDocument::new("SasCompta Devis", Mm(210.0), Mm(297.0), "L1");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("PDF font: {e}"))?;
    let mut layer = doc.get_page(page1).get_layer(layer1);
    let mut y = 280.0;

    layer.use_text(ascii_safe(&org.name), 14.0, Mm(15.0), Mm(y), &font);
    y -= 6.0;
    if let Some(addr) = &org.address {
        for line in addr.lines() {
            layer.use_text(ascii_safe(line), 9.0, Mm(15.0), Mm(y), &font);
            y -= 4.5;
        }
    }
    layer.use_text(
        ascii_safe(&format!(
            "SIREN {}{}",
            org.siren,
            org.siret
                .as_ref()
                .map(|s| format!(" — SIRET {s}"))
                .unwrap_or_default()
        )),
        8.0,
        Mm(15.0),
        Mm(y),
        &font,
    );
    y -= 12.0;

    layer.use_text(
        ascii_safe(&format!("DEVIS {}", quote.number)),
        16.0,
        Mm(15.0),
        Mm(y),
        &font,
    );
    y -= 7.0;
    layer.use_text(
        ascii_safe(&format!(
            "Date {}  |  Statut {}  |  Client {} ({})",
            quote.issue_date, quote.status, quote.third_party_name, quote.third_party_code
        )),
        9.0,
        Mm(15.0),
        Mm(y),
        &font,
    );
    y -= 5.0;
    if let Some(vu) = &quote.valid_until {
        layer.use_text(
            ascii_safe(&format!("Validite jusqu'au {vu}")),
            9.0,
            Mm(15.0),
            Mm(y),
            &font,
        );
        y -= 5.0;
    }
    y -= 4.0;

    layer.use_text(ascii_safe("Lignes"), 11.0, Mm(15.0), Mm(y), &font);
    y -= 6.0;
    for line in &quote.lines {
        if y < 40.0 {
            let (p, l) = doc.add_page(Mm(210.0), Mm(297.0), "L");
            layer = doc.get_page(p).get_layer(l);
            y = 280.0;
        }
        let qty = line.quantity_milli as f64 / 1000.0;
        let row = format!(
            "{}. {}  {} {} x {} = {} HT  TVA {}%",
            line.line_no,
            line.label,
            qty,
            line.unit,
            cents_fr(line.unit_price_cents),
            cents_fr(line.ht_cents),
            format_rate(line.vat_rate_bp),
        );
        layer.use_text(ascii_safe(&row), 8.0, Mm(15.0), Mm(y), &font);
        y -= 5.0;
    }
    y -= 6.0;
    layer.use_text(
        ascii_safe(&format!(
            "Total HT {}   TVA {}   Total TTC {}",
            cents_fr(quote.ht_cents),
            cents_fr(quote.vat_cents),
            cents_fr(quote.ttc_cents),
        )),
        11.0,
        Mm(15.0),
        Mm(y),
        &font,
    );
    y -= 8.0;
    if let Some(notes) = &quote.notes {
        layer.use_text(ascii_safe(&format!("Notes: {notes}")), 8.0, Mm(15.0), Mm(y), &font);
        y -= 5.0;
    }
    if let Some(legal) = &quote.legal_mentions {
        for chunk in wrap_text(legal, 95) {
            if y < 20.0 {
                break;
            }
            layer.use_text(ascii_safe(&chunk), 7.0, Mm(15.0), Mm(y), &font);
            y -= 4.0;
        }
    }
    y -= 8.0;
    layer.use_text(
        ascii_safe("Document genere par SasCompta — devis commercial, sans valeur de facture."),
        7.0,
        Mm(15.0),
        Mm(y),
        &font,
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    doc.save(&mut BufWriter::new(File::create(path)?))
        .map_err(|e| format!("PDF save: {e}"))?;
    Ok(())
}

fn format_rate(bp: i32) -> String {
    format!("{}.{:02}", bp / 100, bp % 100)
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
    // enrich demo client
    let _ = conn.execute(
        "UPDATE third_party SET siret = '12345678200045', email = 'contact@gamma.fr',
         address = '8 avenue du Marche\n69001 Lyon'
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
    Ok(())
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
        assert_eq!(version, "9");

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
}
