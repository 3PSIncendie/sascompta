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
  siret: {
    term: "SIRET",
    plain:
      "SIREN + 5 chiffres (NIC) = 14 chiffres au total. Il identifie un établissement (siège ou secondaire). Utile sur les factures.",
    example: "Ex. : 123 456 782 00002 — les 9 premiers chiffres = SIREN",
  },
  capital: {
    term: "Capital social",
    plain:
      "Somme apportée par les associés à la création (ou après augmentation). Pour une SAS, le montant est libre ; on le retrouve dans les statuts / Kbis.",
    example: "Ex. : 1 000,00 € ou 10 000,00 €",
  },
  adresse: {
    term: "Adresse du siège",
    plain:
      "Adresse du siège social telle qu’elle figure au Kbis. Elle apparaît sur les documents officiels et le pack de comptes annuels.",
    example: "Ex. : 12 rue des Lilas, 75011 Paris",
  },
  formeJuridique: {
    term: "Forme juridique",
    plain:
      "Type de société. SasCompta cible la SAS (plusieurs associés) et la SASU (associé unique). Le fonctionnement comptable est proche.",
    example: "SAS ou SASU",
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
    term: "Mot de passe société",
    plain:
      "Protège le fichier de votre société sur ce disque. Sans lui, personne ne peut ouvrir la compta. À conserver en lieu sûr.",
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
  saisieGuidee: {
    term: "Saisie guidée",
    plain:
      "Vous indiquez une intention (montant HT, compte, régime TVA). SasCompta propose les lignes 445… et la contrepartie. Utile aussi pour l’autoliquidation BTP. Vous pouvez encore tout modifier.",
    example: "Vente 100 € HT à 20 % → crédit 706 100, crédit 44571 20, débit 411 120",
  },
  montantHt: {
    term: "Montant HT",
    plain:
      "Hors taxes : le prix sans TVA. La TVA se calcule ensuite (ex. 20 % de 100 € = 20 €). Le TTC = HT + TVA.",
    example: "Facture 120 € TTC à 20 % → HT = 100 €",
  },
  contrepartie: {
    term: "Contrepartie",
    plain:
      "Le compte qui « balance » l’opération côté client, fournisseur ou banque. En vente on débite souvent 411 ; en achat on crédite 401.",
    example: "Vente → 411 Clients ; achat → 401 Fournisseurs ; paiement immédiat → 512 Banque",
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
  compteTva: {
    term: "Compte TVA (445…)",
    plain:
      "Cases du plan pour la TVA : 44571 = collectée (ventes), 44566 = déductible (achats), 4452 = TVA due par autoliquidation (BTP / art. 283). La saisie guidée les remplit pour vous.",
    example:
      "Vente TTC → crédit 44571 ; achat TTC → débit 44566 ; sous-traitance → débit 44566 + crédit 4452",
  },
  regimeTvaSaisie: {
    term: "Régime TVA (saisie)",
    plain:
      "Comment traiter la TVA sur cette opération : classique (facturée / récupérable), sans TVA, ou autoliquidation BTP (sous-traitance chantier ou travaux facturés HT).",
    example:
      "Achat sous-traitant chantier → Autoliquidation sous-traitance ; travaux que vous facturez à un assujetti → Autoliquidation chantier",
  },
  autoliquidationBtp: {
    term: "Autoliquidation TVA BTP (art. 283)",
    plain:
      "Pour certains travaux immobiliers entre assujettis, le prestataire facture HT. Le client (donneur d’ordre) calcule lui-même la TVA : il la doit (4452) et, s’il y a droit, la déduit (44566). Mentions obligatoires sur la facture.",
    example:
      "Sous-traitance 1 000 € HT à 20 % → débit 604 1 000, débit 44566 200, crédit 4452 200, crédit 401 1 000",
  },
  sousTraitanceChantier: {
    term: "Sous-traitance chantier",
    plain:
      "Vous faites appel à un sous-traitant sur un chantier. Sa facture est en général HT (autoliquidation) : vous n’êtes pas débiteur du TTC fournisseur, mais vous autoliquidez la TVA.",
    example: "Compte charge usuel : 604 Achats de sous-traitance",
  },
  chantierEnDirect: {
    term: "Chantier en direct / travaux BTP",
    plain:
      "Vous réalisez vous-même les travaux pour un client assujetti. Vous facturez HT avec mention d’autoliquidation : pas de TVA collectée (44571) de votre côté ; le client s’en charge.",
    example: "Vente 5 000 € HT → crédit 706 5 000, débit 411 5 000 (code T…R)",
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
  numerotationFactures: {
    term: "Numérotation des factures",
    plain:
      "Chaque facture émise reçoit un numéro unique et chronologique (ex. FACT-2026-0042). Si vous avez déjà facturé hors SasCompta, indiquez ici le prochain numéro à utiliser pour continuer la série sans trou ni doublon.",
    example: "Dernière facture papier FACT-2026-0041 → prochain numéro = 42",
  },
  prochainNumero: {
    term: "Prochain numéro",
    plain:
      "Le numéro qui sera attribué à la prochaine émission de cette série. SasCompta refuse une valeur trop basse si un document portant ce numéro (ou un numéro supérieur) existe déjà dans le fichier.",
    example: "Série FACT, exercice 2026 → aperçu FACT-2026-0042",
  },
};

export function helpPlain(key: keyof typeof HELP): string {
  return HELP[key].plain;
}
