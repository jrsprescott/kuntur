# Carpeta actual en sesiones remotas (OSC 7)

Kuntur sigue la carpeta en la que está cada terminal para tres cosas:

- la **línea inferior** (`usuario@host:/var/www/sitio`);
- la **terminal acoplada** (Ctrl+`), que se abre en la misma carpeta, por otro canal de la misma conexión SSH (sin volver a autenticar);
- la **reconexión**: si la conexión se cae, Enter (o Ctrl+Shift+R) vuelve a abrir el shell en la última carpeta conocida.

La carpeta la informa **el shell**, con la secuencia OSC 7 (`ESC ] 7 ; file://host/ruta BEL`), la misma que usan Windows Terminal, iTerm2, GNOME Terminal o WezTerm. Muchos sistemas ya la emiten (Fedora y Ubuntu recientes la traen en `/etc/profile.d/vte.sh` para algunos terminales); si no, se agrega con unas líneas en el servidor. **Kuntur nunca modifica el servidor**: estas líneas las pone el usuario si quiere.

Sin OSC 7 todo funciona igual, salvo que la acoplada y la reconexión abren el shell en la carpeta de inicio.

## bash (`~/.bashrc` en el servidor)

```bash
# Kuntur / terminales modernas: informar la carpeta actual (OSC 7).
__osc7() {
  local path="${PWD// /%20}"
  printf '\e]7;file://%s%s\a' "${HOSTNAME}" "${path}"
}
case "$PROMPT_COMMAND" in
  *__osc7*) ;;
  *) PROMPT_COMMAND="__osc7${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
esac
```

(bash 5.1+ también acepta `PROMPT_COMMAND+=(__osc7)`.)

## zsh (`~/.zshrc` en el servidor)

```zsh
# Kuntur / terminales modernas: informar la carpeta actual (OSC 7).
__osc7() {
  printf '\e]7;file://%s%s\a' "${HOST}" "${PWD// /%20}"
}
autoload -Uz add-zsh-hook
add-zsh-hook chpwd __osc7
__osc7   # la carpeta inicial
```

Con **Oh My Zsh** basta con no desactivar `termsupport` (ya emite OSC 7 en muchos temas).

## fish

fish 3.x ya emite OSC 7 por su cuenta: no hace falta nada.

## PowerShell (en un servidor Linux con `pwsh`, o en Windows por SSH)

En el perfil del servidor (`$PROFILE`):

```powershell
# Kuntur / terminales modernas: informar la carpeta actual (OSC 7).
$global:__kunturPromptOriginal = $function:prompt
function global:prompt {
    $loc = $executionContext.SessionState.Path.CurrentLocation
    if ($loc.Provider.Name -eq 'FileSystem') {
        $ruta = $loc.ProviderPath -replace '\\', '/' -replace ' ', '%20'
        if ($ruta -notmatch '^/') { $ruta = "/$ruta" }   # C:/x → /C:/x
        $e = [char]27; $bel = [char]7
        [Console]::Write("$e]7;file://$([Environment]::MachineName)$ruta$bel")
    }
    & $global:__kunturPromptOriginal
}
```

En **sesiones locales** no hace falta nada: Kuntur ya envuelve el `prompt` de PowerShell para informar la carpeta (con OSC 9;9, como Windows Terminal) sin cambiar lo que se ve.

## Comprobar

En la sesión remota, después de `cd /tmp`, la línea inferior de Kuntur debe decir `usuario@host:/tmp`. A mano:

```bash
printf '\e]7;file://%s%s\a' "$HOSTNAME" "$PWD"; echo listo
```

## Notas

- Las rutas con espacios van con `%20` (las demás letras, incluidas las tildes, se pueden mandar tal cual en UTF-8).
- `tmux` y `screen` no dejan pasar OSC 7 por defecto. En tmux 3.3+: `set -g allow-passthrough on` y envolver la secuencia (`\ePtmux;\e…\e\\`), o no usar tmux para esto.
- La detección de agentes en el servidor (panel izquierdo, gestor de conexiones) corre `command -v` dentro de un shell de inicio de sesión (`$SHELL -lc`), que carga `~/.profile`: si un agente se instaló en una carpeta que solo agrega `~/.bashrc`, no aparecerá. Agregar esa carpeta al `PATH` en `~/.profile` lo resuelve.
