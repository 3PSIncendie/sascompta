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
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
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
        "#,
    )?;
    enrich_billing_line_description(conn)?;
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

fn sequence_kind_for_invoice(kind: &str) -> &'static str {
    match kind {
        "deposit" => "ACMP",
        "credit_note" => "AVO",
        _ => "FACT", // standard | balance
    }
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
    conn.execute(
        "INSERT INTO invoice(
            number, kind, third_party_id, quote_id, parent_invoice_id, status,
            issue_date, due_date, service_date, ht_cents, vat_cents, ttc_cents, paid_cents,
            vat_mode, vat_code, notes, legal_mentions, created_at, updated_at
         ) VALUES (NULL, ?1, ?2, ?3, ?4, 'draft', ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, ?12, ?13, ?14, ?15, ?15)",
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
    conn.execute(
        "UPDATE invoice SET kind = ?1, third_party_id = ?2, quote_id = ?3, parent_invoice_id = ?4,
            issue_date = ?5, due_date = ?6, service_date = ?7,
            ht_cents = ?8, vat_cents = ?9, ttc_cents = ?10, vat_mode = ?11, vat_code = ?12,
            notes = ?13, legal_mentions = ?14, updated_at = ?15
         WHERE id = ?16",
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

/// Émet la facture : réserve le numéro (FACT/ACMP/AVO) + statut issued + brouillon VTE.
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
    let fy_id = open_fy_id(conn)?;
    let seq = sequence_kind_for_invoice(&existing.kind);
    let number = allocate_document_number(conn, seq, fy_id)?;
    let now = now_iso();
    conn.execute(
        "UPDATE invoice SET number = ?1, status = 'issued', updated_at = ?2 WHERE id = ?3",
        params![number, now, id],
    )?;
    // Max automation : brouillon VTE dès l'émission (post humain ensuite)
    let _ = build_invoice_entry_draft(conn, id)?;
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
        if let Some(desc) = &line.description {
            for chunk in wrap_text(desc, 95) {
                if y < 40.0 {
                    let (p, l) = doc.add_page(Mm(210.0), Mm(297.0), "L");
                    layer = doc.get_page(p).get_layer(l);
                    y = 280.0;
                }
                layer.use_text(ascii_safe(&chunk), 7.0, Mm(20.0), Mm(y), &font);
                y -= 4.0;
            }
        }
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

pub fn export_invoice_pdf(
    conn: &Connection,
    invoice_id: i64,
    path: &Path,
) -> DbResult<()> {
    let inv = get_invoice(conn, invoice_id)?;
    let org = crate::db::load_organization(conn)?;
    let title = match inv.kind.as_str() {
        "deposit" => "FACTURE D'ACOMPTE",
        "balance" => "FACTURE DE SOLDE",
        "credit_note" => "AVOIR",
        _ => "FACTURE",
    };
    let number = inv
        .number
        .clone()
        .unwrap_or_else(|| format!("BROUILLON-{}", inv.id));
    let (doc, page1, layer1) = PdfDocument::new("SasCompta Facture", Mm(210.0), Mm(297.0), "L1");
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
        ascii_safe(&format!("{title} {number}")),
        16.0,
        Mm(15.0),
        Mm(y),
        &font,
    );
    y -= 7.0;
    layer.use_text(
        ascii_safe(&format!(
            "Date {}  |  Statut {}  |  Client {} ({})",
            inv.issue_date, inv.status, inv.third_party_name, inv.third_party_code
        )),
        9.0,
        Mm(15.0),
        Mm(y),
        &font,
    );
    y -= 5.0;
    if let Some(due) = &inv.due_date {
        layer.use_text(
            ascii_safe(&format!("Echeance {due}")),
            9.0,
            Mm(15.0),
            Mm(y),
            &font,
        );
        y -= 5.0;
    }
    if let Some(parent) = inv.parent_invoice_id {
        layer.use_text(
            ascii_safe(&format!("Document lie a facture #{parent}")),
            8.0,
            Mm(15.0),
            Mm(y),
            &font,
        );
        y -= 5.0;
    }
    y -= 4.0;
    layer.use_text(ascii_safe("Lignes"), 11.0, Mm(15.0), Mm(y), &font);
    y -= 6.0;
    for line in &inv.lines {
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
        if let Some(desc) = &line.description {
            for chunk in wrap_text(desc, 95) {
                if y < 40.0 {
                    let (p, l) = doc.add_page(Mm(210.0), Mm(297.0), "L");
                    layer = doc.get_page(p).get_layer(l);
                    y = 280.0;
                }
                layer.use_text(ascii_safe(&chunk), 7.0, Mm(20.0), Mm(y), &font);
                y -= 4.0;
            }
        }
    }
    y -= 6.0;
    layer.use_text(
        ascii_safe(&format!(
            "Total HT {}   TVA {}   Total TTC {}",
            cents_fr(inv.ht_cents),
            cents_fr(inv.vat_cents),
            cents_fr(inv.ttc_cents),
        )),
        11.0,
        Mm(15.0),
        Mm(y),
        &font,
    );
    y -= 8.0;
    if let Some(notes) = &inv.notes {
        layer.use_text(ascii_safe(&format!("Notes: {notes}")), 8.0, Mm(15.0), Mm(y), &font);
        y -= 5.0;
    }
    if let Some(legal) = &inv.legal_mentions {
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
        ascii_safe("Document genere par SasCompta."),
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
    conn.execute(
        "UPDATE invoice SET pdf_path = ?1, updated_at = ?2 WHERE id = ?3",
        params![path.display().to_string(), now_iso(), invoice_id],
    )?;
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

/// Enregistre un encaissement : paiement + brouillon BQ (512/411) + lettrage soft.
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
    let entry = ledger::create_draft(conn, &entry_input)?;
    let lettre = soft_lettre_for_invoice(&inv);
    apply_soft_lettre(conn, entry.id, &lettre, &input.paid_on)?;
    // Lettrage soft sur l'écriture VTE liée si présente (même code)
    if let Some(vte_id) = inv.entry_id {
        let _ = apply_soft_lettre(conn, vte_id, &lettre, &input.paid_on);
    }
    conn.execute(
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
    let payment_id = conn.last_insert_rowid();
    refresh_invoice_payment_status(conn, input.invoice_id)?;
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
        assert!(num.starts_with("FACT-2026-"), "{num}");
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
        assert!(credit_issued
            .number
            .as_deref()
            .unwrap()
            .starts_with("AVO-2026-"));

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
        assert!(dep_issued
            .number
            .as_deref()
            .unwrap()
            .starts_with("ACMP-2026-"));

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
        assert!(pay.lettre.as_ref().unwrap().contains("FACT"));
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
        let journal: String = conn
            .query_row(
                "SELECT j.code FROM entry e JOIN journal j ON j.id = e.journal_id WHERE e.id = ?1",
                params![bq_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(journal, "BQ");

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
        let other = create_invoice(
            &conn,
            &CreateInvoiceInput {
                kind: "standard".into(),
                third_party_id: issued.third_party_id,
                quote_id: None,
                parent_invoice_id: None,
                issue_date: "2025-12-01".into(),
                due_date: Some("2025-12-15".into()),
                service_date: None,
                vat_mode: "normale".into(),
                notes: None,
                legal_mentions: None,
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
}
