/** Glossaire FR en langage simple — aide au remplissage pour débutants. */

export type HelpTopic = {
  term: string;
  plain: string;
  example?: string;
};

export const HELP: Record<string, HelpTopic> = {
  raisonSociale: {
    term: "Raison sociale",
    plain:
      "Le nom officiel de votre SAS, tel qu’il apparaît au Kbis. C’est le nom de la société, pas votre prénom.",
    example: "Ex. : Atelier Lumière SAS",
  },
  siren: {
    term: "SIREN",
    plain:
      "Numéro à 9 chiffres qui identifie votre société auprès de l’administration. On le trouve sur le Kbis.",
    example: "Ex. : 123 456 782 (sans espaces dans le champ)",
  },
  exercice: {
    term: "Exercice comptable",
    plain:
      "La période de 12 mois sur laquelle vous tenez les comptes (souvent du 1er janvier au 31 décembre).",
    example: "Ex. : 2026 = du 01/01/2026 au 31/12/2026",
  },
  profilLocal: {
    term: "Profil local",
    plain:
      "Le prénom ou le rôle de la personne qui utilise ce fichier sur cet ordinateur. Ce n’est pas un compte cloud.",
    example: "Ex. : Marie, Dirigeant, Expert-comptable",
  },
  motDePasse: {
    term: "Mot de passe workspace",
    plain:
      "Protège le fichier .sascompta sur votre disque. Sans lui, personne ne peut ouvrir la compta. À conserver en lieu sûr.",
  },
  planComptable: {
    term: "Plan comptable (PCG)",
    plain:
      "La liste des « cases » où l’on range l’argent : banque, clients, charges, ventes… Chaque case a un numéro (ex. 512 = banque).",
    example: "Classe 1 = capitaux, 2 = immobilisations, 4 = tiers, 5 = banque, 6 = charges, 7 = produits",
  },
  numeroCompte: {
    term: "Numéro de compte",
    plain:
      "Code du Plan comptable général. Plus le numéro est précis, plus le suivi est fin (512 Banque, 6061 Fournitures…).",
    example: "Ex. : 401 = fournisseurs, 411 = clients, 706 = prestations",
  },
  typeCompte: {
    term: "Type de compte",
    plain:
      "Indique si le compte appartient au bilan (actif, passif, capitaux) ou au résultat (charge / produit).",
  },
  journal: {
    term: "Journal",
    plain:
      "Le « cahier » dans lequel on range l’écriture : banque (BQ), ventes (VE), achats (HA), opérations diverses (OD)…",
    example: "Une facture client → journal VE ; un virement bancaire → journal BQ",
  },
  piece: {
    term: "Référence pièce",
    plain:
      "Le numéro du document justificatif (facture, relevé). Utile pour retrouver la preuve en cas de contrôle.",
    example: "Ex. : FA-2026-0042",
  },
  libelle: {
    term: "Libellé",
    plain: "Une phrase courte qui décrit l’opération, compréhensible dans six mois.",
    example: "Ex. : Facture prestation Acme — mai 2026",
  },
  debitCredit: {
    term: "Débit / Crédit",
    plain:
      "En partie double, chaque mouvement a deux côtés qui doivent totaliser la même somme. L’écart doit être à zéro avant validation.",
    example: "Achat 120 € TTC : débit charges + TVA, crédit fournisseur 120 €",
  },
  tva: {
    term: "TVA",
    plain:
      "Taxe collectée pour l’État sur les ventes, et récupérable sur les achats. Le solde (collectée − déductible) se déclare périodiquement.",
  },
  codeTva: {
    term: "Code TVA",
    plain:
      "Étiquette sur une ligne d’écriture pour indiquer le taux et le sens (collectée / déductible). Sert au brouillon CA3.",
    example: "Ex. : T20C = collectée 20 %, T20D = déductible 20 %",
  },
  ca3: {
    term: "CA3",
    plain:
      "Déclaration de TVA réelle. SasCompta prépare un brouillon à partir des écritures ; la déclaration officielle se fait sur le site des impôts.",
  },
  regimeTva: {
    term: "Régime de TVA",
    plain:
      "Fréquence de déclaration : mensuelle, trimestrielle, ou franchise en base (pas de TVA facturée sous seuil).",
  },
  tiers: {
    term: "Tiers",
    plain:
      "Un client ou un fournisseur suivi à part. Utile pour savoir qui vous doit de l’argent, ou à qui vous devez.",
  },
  bilan: {
    term: "Bilan",
    plain:
      "Photo du patrimoine à une date : ce que la société possède (actif) et ce qu’elle doit / possède en capitaux (passif).",
  },
  compteResultat: {
    term: "Compte de résultat",
    plain:
      "Résume les charges et les produits de l’exercice. La différence est le bénéfice ou la perte.",
  },
  cloture: {
    term: "Clôture",
    plain:
      "Fin officielle de l’exercice : on vérifie, on reporte les soldes, on prépare le dépôt des comptes. Irreversible une fois faite.",
  },
  fec: {
    term: "FEC",
    plain:
      "Fichier des Écritures Comptables exigé par l’administration en cas de contrôle. SasCompta exporte un brouillon FEC.",
  },
  depotComptes: {
    term: "Dépôt des comptes",
    plain:
      "Obligation de déposer bilan / compte de résultat / annexe au greffe (souvent via le guichet unique) après l’assemblée d’approbation.",
  },
};

export function helpPlain(key: keyof typeof HELP): string {
  return HELP[key].plain;
}
