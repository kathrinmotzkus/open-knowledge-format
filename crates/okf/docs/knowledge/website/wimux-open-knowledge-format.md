---
title: Open Knowledge Format
type: WebsitePage
kind: project-subpage
topic: okf
status: published
updated: 2026-07-18
language: de
public: true
website:
  model: subpage
  project: okf-wissensdokumentation-fuer-markdown-repositories
  slug: okf
  order: 20
---

# Open Knowledge Format

Das Open Knowledge Format beschreibt, wie gewöhnliche Wissensordner so
strukturiert werden können, dass Menschen und Programme sie gemeinsam nutzen
können. Ein OKF-Repository besteht nicht aus einem proprietären Container,
sondern aus normalen Dateien und Ordnern.

Ein typisches OKF-Dokument ist eine Markdown-Datei mit optionalem YAML
Frontmatter. Dort können Titel, Dokumenttyp, Status, Thema, Aktualisierungsdatum
und Relationen stehen. Der Text bleibt normaler Markdown und ist damit auch ohne
OKF lesbar.

OKF legt besonderen Wert auf klare Grenzen:

- Lokale Dokumente bleiben die kanonische Quelle.
- SQLite-Dateien, Embeddings, Suchindexe und KI-Vorschläge sind abgeleiteter
  Zustand.
- Verbindungen, die eine KI vorschlägt, sind nicht automatisch Wahrheit.
- Externe Quellen werden mit Herkunft und Status sichtbar gemacht.

Dadurch eignet sich OKF für technische Dokumentation, Forschungsnotizen,
Projektwissen, persönliche Wissenssammlungen und später auch für Verbindungen
zu offenen Wissensgraphen wie Wikidata.

Das URI-Schema `okf://` ist als Draft vorbereitet. Es soll Wissensressourcen
adressieren, zum Beispiel:

```text
okf://wikidata/entity/Q794
```

Eine solche URI identifiziert eine konkrete Wissensentität. Suchen,
Filterungen, Zählungen und SPARQL-Abfragen gehören dagegen zur SCQL-Seite.
