import type {
  EinvoicePartyProfile,
  EinvoiceQueueItem,
  EreportingBatch,
} from "../../lib/types";

export const OBLIGATION_PHASES = [
  {
    id: "receive_only",
    label: "Réception seule (phase anticipée)",
  },
  {
    id: "emit_large",
    label: "Émission — grandes / ETI (calendrier paramétrable)",
  },
  {
    id: "emit_all",
    label: "Émission — toutes tailles (y compris PME)",
  },
  {
    id: "franchise_receive",
    label: "Franchise en base — réception + explications",
  },
] as const;

export function phaseLabel(phase: string): string {
  return OBLIGATION_PHASES.find((p) => p.id === phase)?.label ?? phase;
}

export function queueStatusLabel(status: string): string {
  const map: Record<string, string> = {
    received: "Reçu",
    validated: "Validé",
    queued: "En file",
    local_ready: "Prêt local",
    sent: "Envoyé (stub)",
    acked: "Ack stub",
    refused: "Refusé",
    error: "Erreur",
  };
  return map[status] ?? status;
}

export function previewEinvoiceProfile(): EinvoicePartyProfile {
  return {
    organizationId: 1,
    siret: "12345678200002",
    siren: "123456782",
    vatNumber: "FR123456782",
    legalName: "Atelier Lumière SAS",
    addressStructured: "12 rue des Lilas, 75011 Paris",
    routingScheme: "SIRET",
    routingId: "12345678200002",
    obligationPhase: "receive_only",
    obligationReceiveFrom: "2026-09-01",
    obligationEmitFrom: "2027-09-01",
    pdpProvider: "StubPDP (démo)",
    pdpApiBase: "https://example.invalid/pdp",
    pdpTechId: "TECH-DEMO",
    certificateRef: null,
  };
}

export function previewEinvoiceQueue(): EinvoiceQueueItem[] {
  return [
    {
      id: 1,
      direction: "outbound",
      invoiceId: 2,
      entryId: null,
      payloadPath: "/tmp/FACT-2026-0001-factur-x.xml",
      format: "facturx",
      lifecycleStatus: "queued",
      lastError: null,
      attempts: 0,
      summary: "Factur-X BASIC local — FACT-2026-0001 (XML+PDF, pas de dépôt réseau)",
      createdAt: "2026-04-02T10:00:00Z",
      updatedAt: "2026-04-02T10:00:00Z",
    },
    {
      id: 2,
      direction: "inbound",
      invoiceId: null,
      entryId: 88,
      payloadPath: "/tmp/supplier-cii.xml",
      format: "cii",
      lifecycleStatus: "validated",
      lastError: null,
      attempts: 0,
      summary: "Import cii — FA-FOURN / Fournisseur Delta — ACH brouillon #88",
      createdAt: "2026-04-03T09:00:00Z",
      updatedAt: "2026-04-03T09:00:00Z",
    },
  ];
}

export function previewEreportingBatches(): EreportingBatch[] {
  return [
    {
      id: 1,
      periodFrom: "2026-01-01",
      periodTo: "2026-03-31",
      kind: "payments",
      status: "exported",
      payloadPath: "/tmp/ereporting-q1.json",
      entryCount: 0,
      createdAt: "2026-04-01T12:00:00Z",
    },
  ];
}
