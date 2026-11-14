# Security policy

## Reporting a vulnerability

Report privately through GitHub: open the repository's Security tab and choose "Report a
vulnerability" (private vulnerability reporting on `agync/nocturne`). Do not open a public issue or
pull request for it. Say which version you ran, your operating system, and the steps to reproduce.

## In scope

- The desktop app and its Rust core.
- The supervision of the embedded Lemonade engine: its loopback binding, per-launch key and
  configuration.
- The connection helpers shipped for Away from home, and the phone app.
- The release workflows and the packages they publish.

Vulnerabilities in Lemonade itself, Tauri or a connection provider's service belong with those
projects, unless Nocturne's use of them is the cause.

## Design facts that matter for a report

- Nothing leaves the computer except model and app downloads the person agreed to. A report that
  shows other outbound traffic from a default install is a vulnerability.
- The embedded engine listens on loopback only and requires a key made at every launch.
- Away from home and Online answers are off by default. When on, the first sends sealed traffic to
  the person's own paired phones through the connection they chose, and the second sends a question
  with a few remembered sentences to the one provider they chose, never anything marked private.
- Data is kept in one SQLite file in the person's data folder. Deletion is explicit.

See [Your data](docs/guide/your-data.md) and [Architecture](docs/architecture/index.md).

## Supported versions

Only the latest release is supported.
