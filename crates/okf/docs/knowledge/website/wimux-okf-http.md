---
title: OKF-Server
type: WebsitePage
kind: project-subpage
topic: okf-http
status: published
updated: 2026-07-18
language: de
public: true
website:
  model: subpage
  project: okf-wissensdokumentation-fuer-markdown-repositories
  slug: okf-http
  order: 10
---

# OKF-Server

`okf-http` ist der lokale Webserver und Browser für OKF. Er macht
OKF-Repositories im Webbrowser sichtbar, ohne dass dafür ein Cloud-Dienst oder
eine zentrale Datenbank nötig ist.

Im normalen Startmodus ist `okf-http` read-only. Dokumente, Metadaten,
Graphansicht und tokenfreie Planungsinformationen können gelesen werden, aber
Konfigurationsänderungen, Schreibzugriffe und KI-Aufrufe sind deaktiviert.
Geschützte Workflows wie das Hinzufügen neuer Dokumentwurzeln, das Schreiben
akzeptierter Relationen oder kostenpflichtige KI-Analysen müssen ausdrücklich
aktiviert und autorisiert werden.

Installieren lässt sich der Server über crates.io:

```bash
cargo install okf-http --locked
okf-http --install-browser
okf-http 8003
```

Danach öffnet man lokal:

```text
http://127.0.0.1:8003/docs-browser/index.html
```

Die Webseite von OKF nutzt dieses Prinzip selbst: OKF-Dokumente sind die
redaktionelle Quelle, während die öffentliche Website daraus kontrolliert
veröffentlichte Seiten erzeugt.
