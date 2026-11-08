interface Props {
  title: string;
  group: string;
  blurb: string;
}

/** Stub léger M0/M1 — modules Commercial / RH / E-facture pas encore branchés. */
export function ComingSoonScreen({ title, group, blurb }: Props) {
  return (
    <div className="stack coming-soon">
      <header className="page-head">
        <div>
          <p className="eyebrow">{group}</p>
          <h1>{title}</h1>
          <p>{blurb}</p>
        </div>
      </header>
      <section className="panel coming-soon-panel">
        <p className="coming-soon-badge" role="status">
          Bientôt disponible
        </p>
        <p>
          Cet écran est un repère de navigation. La logique métier arrivera dans
          les jalons suivants — aucune donnée n’est encore stockée ici.
        </p>
      </section>
    </div>
  );
}
