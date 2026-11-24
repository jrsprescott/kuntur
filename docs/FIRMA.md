# Firma digital de Kuntur (Authenticode)

Sin firma, Windows SmartScreen muestra "Editor desconocido" o "Windows protegió su PC" al abrir el instalador.
Firmar `kuntur.exe`, `kuntur-hook.exe` y el `.msi` lo evita (con un certificado de buena reputación) y permite que
el actualizador compruebe que una versión nueva viene **del mismo firmante** que la instalada.

Un certificado **autofirmado no sirve**: solo es de confianza en la PC donde se instala a mano.

## Opción 1 · SignPath Foundation (gratis para código abierto)

[SignPath Foundation](https://signpath.org/) firma gratis proyectos de código abierto.

**Requisitos** (ver sus [términos](https://signpath.org/terms)):
- Repositorio **público** con licencia aprobada por la OSI (Kuntur: MIT OR Apache-2.0 ✅).
- Compilación **automática en la CI** (GitHub Actions): lo que se firma tiene que salir del código del repositorio,
  no de una PC.
- Verificación en dos pasos (MFA) en GitHub y en SignPath para todo el equipo.
- Nada de código propietario dentro del proyecto.

**Qué cambia:**
- El **editor** que muestra Windows es **"SignPath Foundation"**, no el nombre del desarrollador.
- Cada versión firmada debe decir: *"Free code signing provided by SignPath.io, certificate by SignPath
  Foundation."* (por ejemplo, en la nota de la versión y en el README).

**Pasos:**
1. Solicitar el ingreso en <https://signpath.org/> (formulario para proyectos de código abierto) con el enlace
   `https://github.com/jrsprescott/kuntur`.
2. Cuando aprueben: en SignPath, crear el proyecto `kuntur`, la política `release-signing` y la configuración de
   artefacto `msi` (firma profunda: los `.exe` dentro del MSI y el MSI).
3. En GitHub → *Settings → Secrets and variables → Actions*, agregar `SIGNPATH_API_TOKEN` y
   `SIGNPATH_ORGANIZATION_ID`.
4. Publicar una etiqueta `vX.Y.Z`: el trabajo `msi` de `.github/workflows/ci.yml` genera el MSI, lo manda a firmar y
   sube el artefacto **`kuntur-msi-firmado`**. Ese es el que se adjunta a la versión.

Sin esos secretos, los pasos de SignPath se saltan solos y la CI sigue igual que antes.

## Opción 2 · Certificado propio (OV, EV o Azure Artifact Signing)

Con un certificado de firma de código instalado en el almacén de Windows (o en su token USB):

```powershell
.\scripts\msi.ps1 -Check -Sign -CertThumbprint <huella de 40 dígitos>
# o: $env:KUNTUR_CERT_THUMBPRINT = '<huella>'; .\scripts\msi.ps1 -Check -Sign
```

El script busca `signtool.exe` en el Windows SDK (o usa la variable `SIGNTOOL`), firma los dos `.exe` **antes** de
armar el MSI y luego el `.msi`, con SHA-256 y sello de tiempo RFC 3161 (`http://timestamp.digicert.com`, cambiable
con `-TimestampUrl`), y verifica cada firma con `signtool verify /pa`.

- **OV:** SmartScreen puede seguir avisando hasta que el archivo gane reputación.
- **EV:** reputación inmediata; viene en un token de hardware.
- **Azure Artifact Signing** (antes *Trusted Signing*): servicio de Microsoft por suscripción, sin token; se firma
  con `signtool` + su complemento (`/dlib`), así que hace falta adaptar `Invoke-Sign` en `msi.ps1`.

La huella y el certificado **nunca** van al repositorio.

## Qué exige el actualizador

`crates/kuntur-update/src/signature.rs`:
- Si el `kuntur.exe` en uso **está firmado**, el instalador descargado debe tener una firma válida **del mismo
  firmante** (nombre del certificado). Si no, no se instala.
- Si el `kuntur.exe` en uso **no está firmado** (0.1 y 0.2), basta el SHA-256 publicado por GitHub; una firma
  **rota** nunca se instala.
- Por eso, al cambiar de firmante (por ejemplo de SignPath a un certificado propio), la primera versión con el
  firmante nuevo hay que instalarla a mano.
