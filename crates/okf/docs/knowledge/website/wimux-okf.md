---
title: OKF – Open Knowledge Format
type: WebsitePage
kind: project-page
topic: okf
status: published
updated: 2026-07-18
language: de
public: true
website:
  model: project
  slug: okf-wissensdokumentation-fuer-markdown-repositories
  short_title: OKF
  teaser: Wissensdokumente, Markdown-Repositories und semantische Verbindungen lokal organisieren.
  order: 0
  show_tile: true
---

# OKF – Open Knowledge Format

OKF ist ein lokales Wissensformat für Menschen, die bereits in Markdown
schreiben und ihre Dokumente trotzdem als zusammenhängende Wissensbasis
verstehen wollen. Es ersetzt keine Schreibumgebung und zwingt Inhalte nicht in
eine Datenbank. Stattdessen liest OKF vorhandene Markdown-Dateien, Frontmatter,
CSV-Ressourcen und Verzeichnisstrukturen aus und macht daraus eine
durchsuchbare, überprüfbare und erweiterbare Knowledge Base.

Der Kern ist bewusst einfach: Wissen bleibt in lesbaren Dateien. Zusätzliche
Struktur entsteht durch Metadaten, stabile Dokumentidentitäten, explizite
Relationen und abgeleitete Indexe. Dadurch können mehrere Dokumentationsordner
zusammen betrachtet werden, ohne sie physisch zu vermischen. Gleichnamige
Dateien wie `index.md` bleiben über ihre gemounteten Wurzeln eindeutig
adressierbar.

OKF besteht derzeit aus zwei veröffentlichten Rust-Komponenten:

- `okf-open-knowledge-format` ist die Bibliothek für OKF-Repositories,
  Dokumente, Frontmatter, Identitäten und sichere Dateiaufnahme.
- `okf-http` ist der lokale Webserver mit Browser, Dokumentübersicht,
  Graphansicht, geschützter Root-Verwaltung und optionaler semantischer
  Analyse.

Optional kann OKF semantische Analyse nutzen, zum Beispiel über Voyage AI. Das
geschieht nicht automatisch beim Lesen der Dokumente. Vorschläge für Kanten
oder andere Beziehungen bleiben als KI-abgeleitete Vorschläge markiert, bis ein
Mensch sie akzeptiert oder ablehnt.

OKF ist damit kein Agenten-Framework und kein Cloud-Data-Format. Es ist eine
lokale Wissensschicht: Dateien, Metadaten, Relationen, Suche und überprüfbare
Ableitungen. Externe Quellen wie Wikidata können künftig als virtuelle
Knowledge Roots eingebunden werden, ohne dass große Datenbestände vollständig
lokal gespeichert werden müssen.

Status: OKF und `okf-http` sind veröffentlicht und nutzbar, befinden sich aber
noch vor Version 1.0. SCQL und die URI-Schemes `okf://` und `scql://` werden
als Drafts vorbereitet.
