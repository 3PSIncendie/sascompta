//! Factur-X local (CII BASIC subset), file d’attente PDP stub, e-reporting brouillon.
//!
//! MVP honnête : génération XML CII + PDF côte à côte (pas de PDF/A-3 hybrid embarqué),
//! import best-effort, « envoi » PDP = simulation locale sans réseau.

use super::billing::{export_invoice_pdf, get_invoice, list_invoices};
use super::ledger;
use super::tax;
use super::DbResult;
use crate::models::*;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::path::{Path, PathBuf};

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn migrate_einvoice(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS einvoice_party_profile (
            organization_id INTEGER PRIMARY KEY CHECK (organization_id = 1),
            siret TEXT,
            siren TEXT,
            vat_number TEXT,
            legal_name TEXT,
            address_structured TEXT,
            routing_scheme TEXT DEFAULT 'SIRET',
            routing_id TEXT,
            obligation_phase TEXT NOT NULL DEFAULT 'receive_only'
                CHECK (obligation_phase IN (
                    'receive_only','emit_large','emit_all','franchise_receive'
                )),
            obligation_receive_from TEXT,
            obligation_emit_from TEXT,
            pdp_provider TEXT,
            pdp_api_base TEXT,
            pdp_tech_id TEXT,
            certificate_ref TEXT
        );

        CREATE TABLE IF NOT EXISTS einvoice_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            direction TEXT NOT NULL CHECK (direction IN ('outbound','inbound')),
            invoice_id INTEGER REFERENCES invoice(id),
            entry_id INTEGER REFERENCES entry(id),
            payload_path TEXT,
            format TEXT NOT NULL DEFAULT 'facturx'
                CHECK (format IN ('facturx','ubl','cii')),
            lifecycle_status TEXT NOT NULL DEFAULT 'queued'
                CHECK (lifecycle_status IN (
                    'received','validated','queued','local_ready',
                    'sent','acked','refused','error'
                )),
            last_error TEXT,
            attempts INTEGER NOT NULL DEFAULT 0,
            summary TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ereporting_batch (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            period_from TEXT NOT NULL,
            period_to TEXT NOT NULL,
            kind TEXT NOT NULL CHECK (kind IN ('b2c','international','payments')),
            status TEXT NOT NULL DEFAULT 'draft'
                CHECK (status IN ('draft','exported','submitted')),
            payload_path TEXT,
            entry_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );
        "#,
    )?;
    // reporting_category sur factures (best-effort)
    let mut stmt = conn.prepare("PRAGMA table_info(invoice)")?;
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();
    if !cols.iter().any(|c| c == "reporting_category") {
        conn.execute(
            "ALTER TABLE invoice ADD COLUMN reporting_category TEXT NOT NULL DEFAULT 'b2b'",
            [],
        )?;
    }
    Ok(())
}

fn ensure_default_profile(conn: &Connection) -> DbResult<()> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM einvoice_party_profile WHERE organization_id = 1",
        [],
        |r| r.get(0),
    )?;
    if exists > 0 {
        return Ok(());
    }
    let org = crate::db::load_organization(conn)?;
    let vat = format!("FR{}", &org.siren);
    conn.execute(
        "INSERT INTO einvoice_party_profile(
            organization_id, siret, siren, vat_number, legal_name,
            address_structured, routing_scheme, routing_id,
            obligation_phase, obligation_receive_from, obligation_emit_from
         ) VALUES (1, ?1, ?2, ?3, ?4, ?5, 'SIRET', ?1, 'receive_only', '2026-09-01', '2027-09-01')",
        params![org.siret, org.siren, vat, org.name, org.address],
    )?;
    Ok(())
}

fn map_profile(r: &rusqlite::Row<'_>) -> rusqlite::Result<EinvoicePartyProfile> {
    Ok(EinvoicePartyProfile {
        organization_id: r.get(0)?,
        siret: r.get(1)?,
        siren: r.get(2)?,
        vat_number: r.get(3)?,
        legal_name: r.get(4)?,
        address_structured: r.get(5)?,
        routing_scheme: r.get(6)?,
        routing_id: r.get(7)?,
        obligation_phase: r.get(8)?,
        obligation_receive_from: r.get(9)?,
        obligation_emit_from: r.get(10)?,
        pdp_provider: r.get(11)?,
        pdp_api_base: r.get(12)?,
        pdp_tech_id: r.get(13)?,
        certificate_ref: r.get(14)?,
    })
}

pub fn get_party_profile(conn: &Connection) -> DbResult<EinvoicePartyProfile> {
    ensure_default_profile(conn)?;
    conn.query_row(
        "SELECT organization_id, siret, siren, vat_number, legal_name, address_structured,
                routing_scheme, routing_id, obligation_phase, obligation_receive_from,
                obligation_emit_from, pdp_provider, pdp_api_base, pdp_tech_id, certificate_ref
         FROM einvoice_party_profile WHERE organization_id = 1",
        [],
        map_profile,
    )
    .map_err(|e| e.into())
}

pub fn upsert_party_profile(
    conn: &Connection,
    input: &UpdateEinvoiceProfileInput,
) -> DbResult<EinvoicePartyProfile> {
    let phase = input.obligation_phase.trim();
    if !matches!(
        phase,
        "receive_only" | "emit_large" | "emit_all" | "franchise_receive"
    ) {
        return Err("Phase d’obligation e-facture invalide".into());
    }
    conn.execute(
        "INSERT INTO einvoice_party_profile(
            organization_id, siret, siren, vat_number, legal_name, address_structured,
            routing_scheme, routing_id, obligation_phase, obligation_receive_from,
            obligation_emit_from, pdp_provider, pdp_api_base, pdp_tech_id, certificate_ref
         ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(organization_id) DO UPDATE SET
            siret = excluded.siret,
            siren = excluded.siren,
            vat_number = excluded.vat_number,
            legal_name = excluded.legal_name,
            address_structured = excluded.address_structured,
            routing_scheme = excluded.routing_scheme,
            routing_id = excluded.routing_id,
            obligation_phase = excluded.obligation_phase,
            obligation_receive_from = excluded.obligation_receive_from,
            obligation_emit_from = excluded.obligation_emit_from,
            pdp_provider = excluded.pdp_provider,
            pdp_api_base = excluded.pdp_api_base,
            pdp_tech_id = excluded.pdp_tech_id,
            certificate_ref = excluded.certificate_ref",
        params![
            input.siret,
            input.siren,
            input.vat_number,
            input.legal_name,
            input.address_structured,
            input.routing_scheme,
            input.routing_id,
            phase,
            input.obligation_receive_from,
            input.obligation_emit_from,
            input.pdp_provider,
            input.pdp_api_base,
            input.pdp_tech_id,
            input.certificate_ref,
        ],
    )?;
    get_party_profile(conn)
}

fn map_queue(r: &rusqlite::Row<'_>) -> rusqlite::Result<EinvoiceQueueItem> {
    Ok(EinvoiceQueueItem {
        id: r.get(0)?,
        direction: r.get(1)?,
        invoice_id: r.get(2)?,
        entry_id: r.get(3)?,
        payload_path: r.get(4)?,
        format: r.get(5)?,
        lifecycle_status: r.get(6)?,
        last_error: r.get(7)?,
        attempts: r.get(8)?,
        summary: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
}

pub fn list_queue(conn: &Connection) -> DbResult<Vec<EinvoiceQueueItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, direction, invoice_id, entry_id, payload_path, format,
                lifecycle_status, last_error, attempts, summary, created_at, updated_at
         FROM einvoice_queue ORDER BY id DESC",
    )?;
    let rows = stmt.query_map([], map_queue)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn cents_to_amount(cents: i64) -> String {
    format!("{:.2}", cents as f64 / 100.0)
}

fn vat_category(vat_mode: &str, rate_bp: i32) -> (&'static str, i32) {
    match vat_mode {
        "sans" => ("E", 0),
        "autoliquidation_chantier" => ("AE", 0),
        _ if rate_bp == 0 => ("Z", 0),
        _ => ("S", rate_bp),
    }
}

/// Construit un XML CII Factur-X BASIC / EN 16931 subset.
pub fn build_cii_xml(conn: &Connection, invoice_id: i64) -> DbResult<String> {
    let inv = get_invoice(conn, invoice_id)?;
    if matches!(inv.status.as_str(), "draft" | "cancelled") {
        return Err("Factur-X réservé aux factures émises".into());
    }
    let profile = get_party_profile(conn)?;
    let org = crate::db::load_organization(conn)?;
    let seller_name = profile
        .legal_name
        .clone()
        .unwrap_or_else(|| org.name.clone());
    let seller_siret = profile
        .siret
        .clone()
        .or(org.siret.clone())
        .unwrap_or_default();
    let seller_vat = profile.vat_number.clone().unwrap_or_default();
    let buyer_siret = inv.third_party_code.clone(); // fallback display; real SIRET from TP
    let tp_siret: Option<String> = conn
        .query_row(
            "SELECT siret FROM third_party WHERE id = ?1",
            params![inv.third_party_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let number = inv
        .number
        .clone()
        .unwrap_or_else(|| format!("FAC-{}", inv.id));
    let issue = &inv.issue_date;
    let due = inv.due_date.clone().unwrap_or_else(|| issue.clone());
    let doc_type = if inv.kind == "credit_note" { "381" } else { "380" };

    let mut lines_xml = String::new();
    for (i, line) in inv.lines.iter().enumerate() {
        let (cat, rate) = vat_category(&inv.vat_mode, line.vat_rate_bp);
        let qty = line.quantity_milli as f64 / 1000.0;
        lines_xml.push_str(&format!(
            r#"
        <ram:IncludedSupplyChainTradeLineItem>
          <ram:AssociatedDocumentLineDocument>
            <ram:LineID>{}</ram:LineID>
          </ram:AssociatedDocumentLineDocument>
          <ram:SpecifiedTradeProduct>
            <ram:Name>{}</ram:Name>
          </ram:SpecifiedTradeProduct>
          <ram:SpecifiedLineTradeAgreement>
            <ram:NetPriceProductTradePrice>
              <ram:ChargeAmount>{}</ram:ChargeAmount>
            </ram:NetPriceProductTradePrice>
          </ram:SpecifiedLineTradeAgreement>
          <ram:SpecifiedLineTradeDelivery>
            <ram:BilledQuantity unitCode="C62">{:.3}</ram:BilledQuantity>
          </ram:SpecifiedLineTradeDelivery>
          <ram:SpecifiedLineTradeSettlement>
            <ram:ApplicableTradeTax>
              <ram:TypeCode>VAT</ram:TypeCode>
              <ram:CategoryCode>{}</ram:CategoryCode>
              <ram:RateApplicablePercent>{:.2}</ram:RateApplicablePercent>
            </ram:ApplicableTradeTax>
            <ram:SpecifiedTradeSettlementLineMonetarySummation>
              <ram:LineTotalAmount>{}</ram:LineTotalAmount>
            </ram:SpecifiedTradeSettlementLineMonetarySummation>
          </ram:SpecifiedLineTradeSettlement>
        </ram:IncludedSupplyChainTradeLineItem>"#,
            i + 1,
            xml_escape(&line.label),
            cents_to_amount(line.unit_price_cents),
            qty,
            cat,
            rate as f64 / 100.0,
            cents_to_amount(line.ht_cents),
        ));
    }

    let (cat, rate) = vat_category(
        &inv.vat_mode,
        inv.lines.first().map(|l| l.vat_rate_bp).unwrap_or(2000),
    );

    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rsm:CrossIndustryInvoice
  xmlns:rsm="urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100"
  xmlns:ram="urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100"
  xmlns:udt="urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100">
  <rsm:ExchangedDocumentContext>
    <ram:GuidelineSpecifiedDocumentContextParameter>
      <ram:ID>urn:cen.eu:en16931:2017#compliant#urn:factur-x.eu:1p0:basic</ram:ID>
    </ram:GuidelineSpecifiedDocumentContextParameter>
  </rsm:ExchangedDocumentContext>
  <rsm:ExchangedDocument>
    <ram:ID>{number}</ram:ID>
    <ram:TypeCode>{doc_type}</ram:TypeCode>
    <ram:IssueDateTime>
      <udt:DateTimeString format="102">{ymd}</udt:DateTimeString>
    </ram:IssueDateTime>
  </rsm:ExchangedDocument>
  <rsm:SupplyChainTradeTransaction>
{lines_xml}
    <ram:ApplicableHeaderTradeAgreement>
      <ram:SellerTradeParty>
        <ram:Name>{seller}</ram:Name>
        <ram:SpecifiedLegalOrganization>
          <ram:ID schemeID="0002">{siret}</ram:ID>
        </ram:SpecifiedLegalOrganization>
        <ram:SpecifiedTaxRegistration>
          <ram:ID schemeID="VA">{vat}</ram:ID>
        </ram:SpecifiedTaxRegistration>
      </ram:SellerTradeParty>
      <ram:BuyerTradeParty>
        <ram:Name>{buyer}</ram:Name>
        <ram:SpecifiedLegalOrganization>
          <ram:ID schemeID="0002">{buyer_id}</ram:ID>
        </ram:SpecifiedLegalOrganization>
      </ram:BuyerTradeParty>
    </ram:ApplicableHeaderTradeAgreement>
    <ram:ApplicableHeaderTradeDelivery/>
    <ram:ApplicableHeaderTradeSettlement>
      <ram:InvoiceCurrencyCode>EUR</ram:InvoiceCurrencyCode>
      <ram:ApplicableTradeTax>
        <ram:CalculatedAmount>{vat_amt}</ram:CalculatedAmount>
        <ram:TypeCode>VAT</ram:TypeCode>
        <ram:CategoryCode>{cat}</ram:CategoryCode>
        <ram:RateApplicablePercent>{rate:.2}</ram:RateApplicablePercent>
        <ram:BasisAmount>{ht}</ram:BasisAmount>
      </ram:ApplicableTradeTax>
      <ram:SpecifiedTradePaymentTerms>
        <ram:DueDateDateTime>
          <udt:DateTimeString format="102">{due_ymd}</udt:DateTimeString>
        </ram:DueDateDateTime>
      </ram:SpecifiedTradePaymentTerms>
      <ram:SpecifiedTradeSettlementHeaderMonetarySummation>
        <ram:LineTotalAmount>{ht}</ram:LineTotalAmount>
        <ram:TaxBasisTotalAmount>{ht}</ram:TaxBasisTotalAmount>
        <ram:TaxTotalAmount currencyID="EUR">{vat_amt}</ram:TaxTotalAmount>
        <ram:GrandTotalAmount>{ttc}</ram:GrandTotalAmount>
        <ram:DuePayableAmount>{due_amt}</ram:DuePayableAmount>
      </ram:SpecifiedTradeSettlementHeaderMonetarySummation>
    </ram:ApplicableHeaderTradeSettlement>
  </rsm:SupplyChainTradeTransaction>
</rsm:CrossIndustryInvoice>
"#,
        number = xml_escape(&number),
        doc_type = doc_type,
        ymd = issue.replace('-', ""),
        lines_xml = lines_xml,
        seller = xml_escape(&seller_name),
        siret = xml_escape(&seller_siret),
        vat = xml_escape(&seller_vat),
        buyer = xml_escape(&inv.third_party_name),
        buyer_id = xml_escape(tp_siret.as_deref().unwrap_or(&buyer_siret)),
        vat_amt = cents_to_amount(inv.vat_cents),
        cat = cat,
        rate = rate as f64 / 100.0,
        ht = cents_to_amount(inv.ht_cents),
        due_ymd = due.replace('-', ""),
        ttc = cents_to_amount(inv.ttc_cents),
        due_amt = cents_to_amount((inv.ttc_cents - inv.paid_cents).max(0)),
    );
    Ok(xml)
}

/// Exporte Factur-X local (XML CII + PDF) et met en file outbound `queued`.
pub fn export_facturx(
    conn: &Connection,
    invoice_id: i64,
    dest_dir: &Path,
) -> DbResult<EinvoiceQueueItem> {
    let inv = get_invoice(conn, invoice_id)?;
    let profile = get_party_profile(conn)?;
    if matches!(
        profile.obligation_phase.as_str(),
        "franchise_receive"
    ) && inv.kind != "credit_note"
    {
        // Franchise : on autorise quand même l’export fichier local (réception prioritaire)
    }
    fs::create_dir_all(dest_dir)?;
    let number = inv
        .number
        .clone()
        .unwrap_or_else(|| format!("FAC-{}", inv.id));
    let safe = number.replace('/', "-");
    let xml_path = dest_dir.join(format!("{safe}-factur-x.xml"));
    let pdf_path = dest_dir.join(format!("{safe}.pdf"));
    let xml = build_cii_xml(conn, invoice_id)?;
    fs::write(&xml_path, xml.as_bytes())?;
    export_invoice_pdf(conn, invoice_id, &pdf_path)?;
    let payload = xml_path.to_string_lossy().to_string();
    let now = now_iso();
    conn.execute(
        "UPDATE invoice SET facturx_path = ?1, pdf_path = ?2,
            einvoice_profile = 'BASIC', einvoice_status = 'local_ready', updated_at = ?3
         WHERE id = ?4",
        params![payload, pdf_path.to_string_lossy().to_string(), now, invoice_id],
    )?;
    let summary = format!(
        "Factur-X BASIC local — {number} → {} (XML+PDF, pas de dépôt réseau)",
        dest_dir.display()
    );
    conn.execute(
        "INSERT INTO einvoice_queue(
            direction, invoice_id, entry_id, payload_path, format,
            lifecycle_status, last_error, attempts, summary, created_at, updated_at
         ) VALUES ('outbound', ?1, NULL, ?2, 'facturx', 'queued', NULL, 0, ?3, ?4, ?4)",
        params![invoice_id, payload, summary, now],
    )?;
    let id = conn.last_insert_rowid();
    list_queue(conn)?
        .into_iter()
        .find(|q| q.id == id)
        .ok_or_else(|| "File e-facture introuvable".into())
}

/// Stub PDP : simule un envoi puis un ack local (aucun appel réseau).
pub fn pdp_stub_submit(conn: &Connection, queue_id: i64) -> DbResult<EinvoiceQueueItem> {
    let profile = get_party_profile(conn)?;
    if profile.pdp_provider.as_deref().unwrap_or("").trim().is_empty() {
        return Err(
            "Configurez un opérateur PDP (stub) dans Ma SAS avant simulation d’envoi".into(),
        );
    }
    let item = list_queue(conn)?
        .into_iter()
        .find(|q| q.id == queue_id)
        .ok_or("Élément de file introuvable")?;
    if item.direction != "outbound" {
        return Err("Seul un flux outbound peut être soumis au stub PDP".into());
    }
    if !matches!(
        item.lifecycle_status.as_str(),
        "queued" | "local_ready" | "error"
    ) {
        return Err(format!(
            "Statut {} non soumissible",
            item.lifecycle_status
        )
        .into());
    }
    let now = now_iso();
    let attempts = item.attempts + 1;
    let summary = format!(
        "STUB PDP « {} » — ack local (aucune transmission réelle). TechId={}",
        profile.pdp_provider.as_deref().unwrap_or("?"),
        profile.pdp_tech_id.as_deref().unwrap_or("n/a")
    );
    conn.execute(
        "UPDATE einvoice_queue SET lifecycle_status = 'acked', attempts = ?1,
            summary = ?2, last_error = NULL, updated_at = ?3 WHERE id = ?4",
        params![attempts, summary, now, queue_id],
    )?;
    if let Some(inv_id) = item.invoice_id {
        conn.execute(
            "UPDATE invoice SET einvoice_status = 'stub_acked', updated_at = ?1 WHERE id = ?2",
            params![now, inv_id],
        )?;
    }
    list_queue(conn)?
        .into_iter()
        .find(|q| q.id == queue_id)
        .ok_or_else(|| "File e-facture introuvable".into())
}

fn extract_tag(xml: &str, local_name: &str) -> Option<String> {
    extract_tag_from(xml, local_name)
}

fn extract_tag_from(xml: &str, local_name: &str) -> Option<String> {
    // Accepte <ram:Foo>, <Foo>, <ram:Foo attr="…">
    let open_prefixed = format!(":{local_name}");
    let open_plain = format!("<{local_name}");
    let mut from = 0usize;
    while from < xml.len() {
        let slice = &xml[from..];
        let hit = slice
            .find(&open_prefixed)
            .map(|i| (i, open_prefixed.len()))
            .or_else(|| slice.find(&open_plain).map(|i| (i, open_plain.len())));
        let Some((rel, marker_len)) = hit else {
            break;
        };
        let abs = from + rel;
        let after_marker = abs + marker_len;
        let Some(gt_rel) = xml[after_marker..].find('>') else {
            break;
        };
        let attrs = &xml[after_marker..after_marker + gt_rel];
        if attrs.contains('<') {
            from = after_marker;
            continue;
        }
        if attrs.trim_end().ends_with('/') {
            from = after_marker + gt_rel + 1;
            continue;
        }
        let content_start = after_marker + gt_rel + 1;
        let close_a = format!("</ram:{local_name}>");
        let close_b = format!("</{local_name}>");
        let end_rel = xml[content_start..]
            .find(&close_a)
            .or_else(|| xml[content_start..].find(&close_b))?;
        let raw = xml[content_start..content_start + end_rel].trim();
        if !raw.contains('<') {
            return Some(raw.to_string());
        }
        if let Some(inner) = raw.rfind('>') {
            let v = raw[inner + 1..].trim();
            if !v.is_empty() && !v.contains('<') {
                return Some(v.to_string());
            }
        }
        from = content_start + end_rel + 1;
    }
    None
}

fn section_between<'a>(xml: &'a str, open_local: &str, close_local: &str) -> Option<&'a str> {
    let open = format!(":{open_local}");
    let pos = xml.find(&open)?;
    let after_open = pos + open.len();
    let gt = xml[after_open..].find('>')? + after_open + 1;
    let close = format!("</ram:{close_local}>");
    let close_alt = format!("</{close_local}>");
    let end = xml[gt..]
        .find(&close)
        .or_else(|| xml[gt..].find(&close_alt))?
        + gt;
    Some(&xml[gt..end])
}

fn parse_amount_to_cents(s: &str) -> i64 {
    let cleaned = s.trim().replace(',', ".").replace(' ', "");
    let v: f64 = cleaned.parse().unwrap_or(0.0);
    (v * 100.0).round() as i64
}

fn detect_format(xml: &str) -> &'static str {
    if xml.contains("CrossIndustryInvoice") || xml.contains("factur-x") {
        if xml.contains("CrossIndustryInvoice") {
            "cii"
        } else {
            "facturx"
        }
    } else if xml.contains("Invoice") && xml.contains("urn:oasis:names:specification:ubl") {
        "ubl"
    } else if xml.contains("CrossIndustryInvoice") {
        "cii"
    } else {
        "cii"
    }
}

struct ParsedInbound {
    number: String,
    seller_name: String,
    seller_siret: Option<String>,
    issue_date: String,
    ht_cents: i64,
    vat_cents: i64,
    ttc_cents: i64,
    line_label: String,
}

fn parse_inbound_xml(xml: &str) -> DbResult<ParsedInbound> {
    let doc = section_between(xml, "ExchangedDocument", "ExchangedDocument").unwrap_or(xml);
    let number = extract_tag_from(doc, "ID")
        .filter(|s| !s.starts_with("urn:"))
        .or_else(|| extract_between(xml, "<cbc:ID>", "</cbc:ID>"))
        .unwrap_or_else(|| "IMPORT".into());

    let seller_sec = section_between(xml, "SellerTradeParty", "SellerTradeParty").unwrap_or("");
    let seller = extract_tag_from(seller_sec, "Name")
        .or_else(|| extract_between(xml, "<cbc:Name>", "</cbc:Name>"))
        .unwrap_or_else(|| "Fournisseur import".into());
    let siret = extract_tag_from(seller_sec, "ID")
        .filter(|s| s.chars().all(|c| c.is_ascii_digit()) && (s.len() == 14 || s.len() == 9));

    let issue_raw = extract_tag_from(doc, "DateTimeString")
        .or_else(|| extract_tag(xml, "DateTimeString"))
        .unwrap_or_else(|| "20260101".into());
    let issue_date = if issue_raw.len() == 8 && issue_raw.chars().all(|c| c.is_ascii_digit()) {
        format!(
            "{}-{}-{}",
            &issue_raw[0..4],
            &issue_raw[4..6],
            &issue_raw[6..8]
        )
    } else {
        issue_raw
    };

    let header = section_between(
        xml,
        "SpecifiedTradeSettlementHeaderMonetarySummation",
        "SpecifiedTradeSettlementHeaderMonetarySummation",
    )
    .unwrap_or(xml);
    let ht = extract_tag_from(header, "TaxBasisTotalAmount")
        .or_else(|| extract_tag_from(header, "LineTotalAmount"))
        .or_else(|| extract_between(xml, "<cbc:TaxExclusiveAmount>", "</cbc:TaxExclusiveAmount>"))
        .map(|s| parse_amount_to_cents(&s))
        .unwrap_or(0);
    let vat = extract_tag_from(header, "TaxTotalAmount")
        .or_else(|| {
            section_between(xml, "ApplicableTradeTax", "ApplicableTradeTax")
                .and_then(|s| extract_tag_from(s, "CalculatedAmount"))
        })
        .or_else(|| extract_between(xml, "<cbc:TaxAmount>", "</cbc:TaxAmount>"))
        .map(|s| parse_amount_to_cents(&s))
        .unwrap_or(0);
    let ttc = extract_tag_from(header, "GrandTotalAmount")
        .or_else(|| extract_between(xml, "<cbc:PayableAmount>", "</cbc:PayableAmount>"))
        .map(|s| parse_amount_to_cents(&s))
        .unwrap_or(ht + vat);

    let line_sec = section_between(
        xml,
        "IncludedSupplyChainTradeLineItem",
        "IncludedSupplyChainTradeLineItem",
    )
    .unwrap_or("");
    let line_label = extract_tag_from(line_sec, "Name")
        .filter(|n| n != &seller)
        .unwrap_or_else(|| format!("Achat import {number}"));

    Ok(ParsedInbound {
        number,
        seller_name: seller,
        seller_siret: siret,
        issue_date,
        ht_cents: ht,
        vat_cents: vat,
        ttc_cents: ttc,
        line_label,
    })
}

fn extract_between(hay: &str, open: &str, close: &str) -> Option<String> {
    let start = hay.find(open)? + open.len();
    let end = hay[start..].find(close)? + start;
    Some(hay[start..end].trim().to_string())
}

fn ensure_supplier(
    conn: &Connection,
    name: &str,
    siret: Option<&str>,
) -> DbResult<(i64, String)> {
    if let Some(s) = siret {
        let found: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, code FROM third_party WHERE siret = ?1 AND kind = 'supplier'",
                params![s],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some(pair) = found {
            return Ok(pair);
        }
    }
    let code = format!(
        "F{}",
        conn.query_row(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM third_party",
            [],
            |r| r.get::<_, i64>(0),
        )?
    );
    let tp = tax::create_third_party(
        conn,
        &CreateThirdPartyInput {
            code: code.clone(),
            name: name.into(),
            kind: "supplier".into(),
            siret: siret.map(|s| s.to_string()),
            email: None,
            address: None,
            routing_id: None,
        },
    )?;
    Ok((tp.id, tp.code))
}

/// Import CII / UBL / Factur-X XML → fournisseur + brouillon ACH + file inbound.
pub fn import_structured_invoice(
    conn: &Connection,
    path: &Path,
) -> DbResult<EinvoiceQueueItem> {
    let raw = fs::read_to_string(path).map_err(|e| format!("Lecture fichier: {e}"))?;
    let format = detect_format(&raw);
    let parsed = parse_inbound_xml(&raw)?;
    if parsed.ttc_cents <= 0 && parsed.ht_cents <= 0 {
        return Err("Montants introuvables dans le fichier importé".into());
    }
    let ht = if parsed.ht_cents > 0 {
        parsed.ht_cents
    } else {
        parsed.ttc_cents - parsed.vat_cents
    };
    let vat = parsed.vat_cents;
    let ttc = if parsed.ttc_cents > 0 {
        parsed.ttc_cents
    } else {
        ht + vat
    };
    let (tp_id, tp_code) = ensure_supplier(
        conn,
        &parsed.seller_name,
        parsed.seller_siret.as_deref(),
    )?;
    let mut lines = vec![EntryLineInput {
        account_number: "607".into(),
        label: Some(parsed.line_label.clone()),
        debit_cents: ht,
        credit_cents: 0,
        vat_code: if vat > 0 {
            Some("T20D".into())
        } else {
            None
        },
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
            vat_code: Some("T20D".into()),
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
    let entry = ledger::create_draft(
        conn,
        &CreateEntryInput {
            journal_code: "ACH".into(),
            entry_date: parsed.issue_date.clone(),
            piece_ref: parsed.number.clone(),
            label: format!("Import e-facture {}", parsed.number),
            lines,
        },
    )?;
    // Copier payload dans workspace-adjacent path string
    let payload = path.to_string_lossy().to_string();
    let now = now_iso();
    let summary = format!(
        "Import {format} — {} / {} — ACH brouillon #{} (TTC {} c)",
        parsed.number, parsed.seller_name, entry.id, ttc
    );
    conn.execute(
        "INSERT INTO einvoice_queue(
            direction, invoice_id, entry_id, payload_path, format,
            lifecycle_status, last_error, attempts, summary, created_at, updated_at
         ) VALUES ('inbound', NULL, ?1, ?2, ?3, 'validated', NULL, 0, ?4, ?5, ?5)",
        params![entry.id, payload, format, summary, now],
    )?;
    let id = conn.last_insert_rowid();
    let _ = tp_id;
    list_queue(conn)?
        .into_iter()
        .find(|q| q.id == id)
        .ok_or_else(|| "File inbound introuvable".into())
}

pub fn list_ereporting_batches(conn: &Connection) -> DbResult<Vec<EreportingBatch>> {
    let mut stmt = conn.prepare(
        "SELECT id, period_from, period_to, kind, status, payload_path, entry_count, created_at
         FROM ereporting_batch ORDER BY id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(EreportingBatch {
            id: r.get(0)?,
            period_from: r.get(1)?,
            period_to: r.get(2)?,
            kind: r.get(3)?,
            status: r.get(4)?,
            payload_path: r.get(5)?,
            entry_count: r.get(6)?,
            created_at: r.get(7)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Construit un brouillon e-reporting (agrégats locaux) et exporte CSV ou JSON.
pub fn build_ereporting_batch(
    conn: &Connection,
    input: &CreateEreportingBatchInput,
) -> DbResult<EreportingBatch> {
    let kind = input.kind.trim();
    if !matches!(kind, "b2c" | "international" | "payments") {
        return Err("Type e-reporting invalide (b2c|international|payments)".into());
    }
    let dest = PathBuf::from(&input.dest_path);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut rows: Vec<serde_json::Value> = Vec::new();
    match kind {
        "payments" => {
            let mut stmt = conn.prepare(
                "SELECT p.id, p.paid_on, p.amount_cents, p.method, i.number, tp.code
                 FROM payment p
                 JOIN invoice i ON i.id = p.invoice_id
                 JOIN third_party tp ON tp.id = i.third_party_id
                 WHERE p.paid_on >= ?1 AND p.paid_on <= ?2
                 ORDER BY p.paid_on",
            )?;
            let mapped = stmt.query_map(params![input.period_from, input.period_to], |r| {
                Ok(serde_json::json!({
                    "paymentId": r.get::<_, i64>(0)?,
                    "paidOn": r.get::<_, String>(1)?,
                    "amountCents": r.get::<_, i64>(2)?,
                    "method": r.get::<_, String>(3)?,
                    "invoiceNumber": r.get::<_, Option<String>>(4)?,
                    "thirdPartyCode": r.get::<_, String>(5)?,
                }))
            })?;
            for row in mapped.flatten() {
                rows.push(row);
            }
        }
        "b2c" | "international" => {
            let invoices = list_invoices(conn)?;
            for inv in invoices {
                if inv.issue_date < input.period_from || inv.issue_date > input.period_to {
                    continue;
                }
                if matches!(inv.status.as_str(), "draft" | "cancelled") {
                    continue;
                }
                let cat: String = conn
                    .query_row(
                        "SELECT reporting_category FROM invoice WHERE id = ?1",
                        params![inv.id],
                        |r| r.get(0),
                    )
                    .unwrap_or_else(|_| "b2b".into());
                if cat != kind {
                    continue;
                }
                rows.push(serde_json::json!({
                    "invoiceId": inv.id,
                    "number": inv.number,
                    "issueDate": inv.issue_date,
                    "thirdParty": inv.third_party_name,
                    "htCents": inv.ht_cents,
                    "vatCents": inv.vat_cents,
                    "ttcCents": inv.ttc_cents,
                    "reportingCategory": cat,
                }));
            }
        }
        _ => unreachable!(),
    }

    let count = rows.len() as i64;
    let ext = dest
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("json")
        .to_lowercase();
    if ext == "csv" {
        let mut csv = String::from("kind,period_from,period_to,payload\n");
        for r in &rows {
            csv.push_str(&format!(
                "{},{},{},\"{}\"\n",
                kind,
                input.period_from,
                input.period_to,
                r.to_string().replace('"', "'")
            ));
        }
        if rows.is_empty() {
            csv.push_str(&format!(
                "{},{},{},\"[]\"\n",
                kind, input.period_from, input.period_to
            ));
        }
        fs::write(&dest, csv)?;
    } else {
        let payload = serde_json::json!({
            "kind": kind,
            "periodFrom": input.period_from,
            "periodTo": input.period_to,
            "disclaimer": "Brouillon e-reporting SasCompta — pas de dépôt PPF/PDP. Dates et champs indicatifs.",
            "entries": rows,
        });
        fs::write(
            &dest,
            serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?,
        )?;
    }

    let now = now_iso();
    conn.execute(
        "INSERT INTO ereporting_batch(period_from, period_to, kind, status, payload_path, entry_count, created_at)
         VALUES (?1, ?2, ?3, 'exported', ?4, ?5, ?6)",
        params![
            input.period_from,
            input.period_to,
            kind,
            dest.to_string_lossy().to_string(),
            count,
            now
        ],
    )?;
    let id = conn.last_insert_rowid();
    list_ereporting_batches(conn)?
        .into_iter()
        .find(|b| b.id == id)
        .ok_or_else(|| "Lot e-reporting introuvable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_demo_workspace, open_connection};
    use crate::db::billing::{issue_invoice, list_invoices};
    use tempfile::tempdir;

    #[test]
    fn facturx_export_import_pdp_stub_and_ereporting() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ei.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        migrate_einvoice(&conn).unwrap();

        let profile = get_party_profile(&conn).unwrap();
        assert_eq!(profile.obligation_phase, "receive_only");

        upsert_party_profile(
            &conn,
            &UpdateEinvoiceProfileInput {
                siret: Some("12345678200002".into()),
                siren: Some("123456782".into()),
                vat_number: Some("FR123456782".into()),
                legal_name: Some("Demo SAS".into()),
                address_structured: Some("1 rue Test".into()),
                routing_scheme: Some("SIRET".into()),
                routing_id: Some("12345678200002".into()),
                obligation_phase: "emit_all".into(),
                obligation_receive_from: Some("2026-09-01".into()),
                obligation_emit_from: Some("2027-01-01".into()),
                pdp_provider: Some("StubPDP".into()),
                pdp_api_base: Some("https://example.invalid/pdp".into()),
                pdp_tech_id: Some("TECH-DEMO".into()),
                certificate_ref: None,
            },
        )
        .unwrap();

        let draft = list_invoices(&conn)
            .unwrap()
            .into_iter()
            .find(|i| i.status == "draft")
            .unwrap();
        let issued = issue_invoice(&conn, draft.id).unwrap();
        let out_dir = dir.path().join("facturx-out");
        let q = export_facturx(&conn, issued.id, &out_dir).unwrap();
        assert_eq!(q.direction, "outbound");
        assert_eq!(q.lifecycle_status, "queued");
        assert!(PathBuf::from(q.payload_path.as_ref().unwrap()).exists());

        let submitted = pdp_stub_submit(&conn, q.id).unwrap();
        assert_eq!(submitted.lifecycle_status, "acked");
        assert!(submitted.attempts >= 1);

        // Réimporter le XML généré
        let xml_path = PathBuf::from(submitted.payload_path.unwrap());
        let inbound = import_structured_invoice(&conn, &xml_path).unwrap();
        assert_eq!(inbound.direction, "inbound");
        assert_eq!(inbound.lifecycle_status, "validated");
        assert!(inbound.entry_id.is_some());

        let batch_path = dir.path().join("ereporting.json");
        let batch = build_ereporting_batch(
            &conn,
            &CreateEreportingBatchInput {
                period_from: "2026-01-01".into(),
                period_to: "2026-12-31".into(),
                kind: "payments".into(),
                dest_path: batch_path.to_string_lossy().to_string(),
            },
        )
        .unwrap();
        assert_eq!(batch.status, "exported");
        assert!(batch_path.exists());
    }
}
