import { useEffect, useId, useRef, useState } from "react";
import { HELP, type HelpTopic } from "../lib/glossary";

type Props = {
  topic: keyof typeof HELP | HelpTopic;
  compact?: boolean;
};

function resolve(topic: Props["topic"]): HelpTopic {
  return typeof topic === "string" ? HELP[topic] : topic;
}

/** Infobulle + panneau d’explication jargon → langage simple. */
export function FieldHelp({ topic, compact }: Props) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const rootRef = useRef<HTMLSpanElement>(null);
  const h = resolve(topic);

  useEffect(() => {
    if (!open) return;
    function onDoc(ev: MouseEvent) {
      if (!rootRef.current?.contains(ev.target as Node)) {
        setOpen(false);
      }
    }
    function onKey(ev: KeyboardEvent) {
      if (ev.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <span
      className={`field-help${compact ? " compact" : ""}${open ? " open" : ""}`}
      ref={rootRef}
    >
      <button
        type="button"
        className="field-help-btn"
        aria-expanded={open}
        aria-controls={id}
        title={`Aide : ${h.term}`}
        onMouseDown={(e) => {
          // Empêche le <label> parent de voler le clic.
          e.preventDefault();
          e.stopPropagation();
        }}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setOpen((v) => !v);
        }}
      >
        ?
      </button>
      {open && (
        <div id={id} className="field-help-panel" role="note">
          <strong>{h.term}</strong>
          <p>{h.plain}</p>
          {h.example && <p className="field-help-example">{h.example}</p>}
          <button
            type="button"
            className="ghost field-help-close"
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              setOpen(false);
            }}
          >
            Fermer
          </button>
        </div>
      )}
    </span>
  );
}

export function LabelWithHelp({
  topic,
  children,
}: {
  topic: keyof typeof HELP;
  children: React.ReactNode;
}) {
  return (
    <span className="label-with-help">
      {children}
      <FieldHelp topic={topic} compact />
    </span>
  );
}
