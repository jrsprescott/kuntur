# Vista previa de Kuntur

Archivo **sintético** para las pruebas de la vista previa (F8). No contiene datos reales.

## Qué se ve aquí

- Títulos, *cursiva*, **negrita** y `código en línea`.
- Listas con viñetas y numeradas.
- Bloques de código y tablas.

1. Ctrl+clic en una ruta de la terminal abre la vista previa.
2. Clic derecho en una ruta muestra el menú.
3. El archivo se recarga solo al cambiar.

```rust
fn main() {
    println!("Hola desde la vista previa");
}
```

| Formato | Visor |
|---|---|
| Markdown | `TextView::markdown` |
| PNG, JPG, GIF, SVG | `img` de GPUI |
| PDF, HTML | WebView2 (`wry`) |

> El diff a colores muestra qué líneas cambió el agente.
