# Code signing policy

**English** · [Español](#política-de-firma-de-código)

> **Status (2026-11-25): releases are not signed yet.** The SignPath Foundation application was not approved (the
> program requires broader public adoption first); the project may apply again later. This policy describes how
> releases will be signed once a certificate is available.

## What is signed

Only release builds of Kuntur Terminal IA Agente: `kuntur.exe`, `kuntur-hook.exe` and the `kuntur-<version>-x86_64.msi`
installer. They are built from this repository by GitHub Actions (`.github/workflows/ci.yml`) when a `v*` tag is
pushed; no binary built on a personal machine is submitted for signing.

## Team roles

| Role | Members |
|---|---|
| Committers and reviewers | [jrsprescott](https://github.com/jrsprescott) |
| Approvers | [jrsprescott](https://github.com/jrsprescott) |

Every signing request is approved manually by an approver. All team members use multi-factor authentication on
GitHub and SignPath.

## Privacy policy

This program will not transfer any information to other networked systems unless specifically requested by the
user or the person installing or operating it, with one exception: it checks the project's GitHub Releases for a
newer version (`api.github.com`, an anonymous request with a generic `Kuntur/<version>` user agent, when it starts
and every 24 hours). That check can be turned off in *Settings → Updates*. Kuntur has no telemetry and no account;
everything else (usage statistics, audit, settings) stays on the user's computer.

Connections the user starts on purpose — SSH/SFTP servers, MCP servers, links opened from the terminal — go only where
the user points them.

---

# Política de firma de código

> **Estado (2026-11-25): las versiones todavía no están firmadas.** SignPath Foundation no aprobó la solicitud (el
> programa pide primero más adopción pública); se puede volver a postular más adelante. Esta política describe cómo
> se firmarán las versiones cuando haya un certificado.

## Qué se firma

Solo las versiones publicadas de Kuntur Terminal IA Agente: `kuntur.exe`, `kuntur-hook.exe` y el instalador
`kuntur-<versión>-x86_64.msi`, compilados desde este repositorio por GitHub Actions al publicar una etiqueta `v*`.
Nunca se manda a firmar un binario compilado en una PC personal.

## Roles

| Rol | Integrantes |
|---|---|
| Autores y revisores | [jrsprescott](https://github.com/jrsprescott) |
| Aprobadores | [jrsprescott](https://github.com/jrsprescott) |

Cada pedido de firma lo aprueba a mano un aprobador. Todo el equipo usa verificación en dos pasos en GitHub y en
SignPath.

## Privacidad

Este programa no envía información a otros sistemas en red salvo que lo pida quien lo usa o lo instala, con una
excepción: consulta las versiones publicadas en GitHub (`api.github.com`, una petición anónima con un agente
genérico `Kuntur/<versión>`, al abrir y cada 24 horas). Se apaga en *Configuración → Actualizaciones*. Kuntur no
tiene telemetría ni cuentas; todo lo demás (estadísticas de uso, auditoría, configuración) queda en la PC.

Las conexiones que el usuario abre a propósito —servidores SSH/SFTP, servidores MCP, enlaces abiertos desde la
terminal— van solo adonde él las dirige.
