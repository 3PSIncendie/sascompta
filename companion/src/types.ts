export type DocTypeHint =
  | "supplier_invoice"
  | "customer_invoice"
  | "customer_quote"
  | "expense_receipt"
  | "ticket"
  | "bank_statement"
  | "unknown";

export type PairingPayload = {
  v: number;
  app?: string;
  host: string;
  port: number;
  code: string;
  org?: string;
  expiresAt?: string;
};

export type Session = {
  host: string;
  port: number;
  token: string;
  orgName: string;
};

export const DOC_TYPE_OPTIONS: { value: DocTypeHint; label: string }[] = [
  { value: "expense_receipt", label: "Note de frais / reçu" },
  { value: "ticket", label: "Ticket de caisse" },
  { value: "supplier_invoice", label: "Facture fournisseur" },
  { value: "customer_invoice", label: "Facture client" },
  { value: "customer_quote", label: "Devis client" },
  { value: "bank_statement", label: "Relevé bancaire" },
  { value: "unknown", label: "Je ne sais pas (PC classera)" },
];
