//! Costo estimado: tabla de precios por modelo (USD por millón de tokens) cargada de
//! `pricing.toml` (la incluida en Kuntur y, encima, la del usuario en `%APPDATA%\Kuntur\`).

use serde::Deserialize;

use crate::{Cost, UsageRecord};

/// Tabla incluida en Kuntur (precios verificados en las páginas oficiales; ver el archivo).
pub const BUILTIN: &str = include_str!("../pricing.toml");

/// Proveedores que sirven modelos en la PC del usuario (ajustes v0.2.1): su uso cuesta cero.
pub const LOCAL_PROVIDERS: [&str; 9] = [
    "lmstudio",
    "lm-studio",
    "ollama",
    "llama.cpp",
    "llamacpp",
    "llama-cpp",
    "jan",
    "localai",
    "local",
];

/// ¿El modelo corre en la PC? Por su prefijo `proveedor/` (como los registra OpenCode) o por una
/// dirección local (`localhost`, `127.0.0.1`).
pub fn is_local_model(model: &str) -> bool {
    let lower = model.trim().to_ascii_lowercase();
    if lower.contains("localhost") || lower.contains("127.0.0.1") {
        return true;
    }
    lower
        .split_once('/')
        .is_some_and(|(vendor, _)| LOCAL_PROVIDERS.contains(&vendor))
}

/// Precio de un modelo, en USD por millón de tokens.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Price {
    #[serde(rename = "modelo")]
    pub model: String,
    #[serde(rename = "entrada")]
    pub input: f64,
    #[serde(rename = "salida")]
    pub output: f64,
    #[serde(rename = "cache_lectura", default)]
    pub cache_read: f64,
    #[serde(rename = "cache_escritura", default)]
    pub cache_write: f64,
}

#[derive(Deserialize)]
struct File {
    #[serde(rename = "precio", default)]
    prices: Vec<Price>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PriceTable {
    prices: Vec<Price>,
}

impl PriceTable {
    /// Lee un `pricing.toml`. Nombres en minúsculas; precios negativos o no finitos son error.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut prices = file.prices;
        for p in &mut prices {
            p.model = p.model.trim().to_ascii_lowercase();
            let values = [p.input, p.output, p.cache_read, p.cache_write];
            if p.model.is_empty() || values.iter().any(|v| !v.is_finite() || *v < 0.0) {
                return Err(format!("precio inválido para \"{}\"", p.model));
            }
        }
        Ok(Self { prices })
    }

    /// La tabla incluida en Kuntur.
    pub fn builtin() -> Self {
        Self::parse(BUILTIN).unwrap_or_default()
    }

    /// `self` con las entradas de `user` encima (las del usuario ganan para el mismo modelo).
    pub fn with_overrides(mut self, user: PriceTable) -> Self {
        self.prices.retain(|p| !user.prices.iter().any(|u| u.model == p.model));
        let mut prices = user.prices;
        prices.append(&mut self.prices);
        Self { prices }
    }

    pub fn len(&self) -> usize {
        self.prices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.prices.is_empty()
    }

    /// Precio de `model`: nombre exacto o seguido de una fecha (`-20251001`), sin mayúsculas e
    /// ignorando un prefijo `proveedor/`.
    pub fn find(&self, model: &str) -> Option<&Price> {
        let lower = model.trim().to_ascii_lowercase();
        let name = lower.rsplit('/').next().unwrap_or(&lower);
        self.prices.iter().find(|p| {
            name == p.model
                || name.strip_prefix(p.model.as_str()).is_some_and(|rest| {
                    rest.strip_prefix('-')
                        .is_some_and(|date| date.len() >= 6 && date.chars().all(|c| c.is_ascii_digit()))
                })
        })
    }

    /// Costo estimado del registro en micro-dólares (`None` si el modelo no tiene precio). Los
    /// modelos locales cuestan cero.
    pub fn estimate(&self, r: &UsageRecord) -> Option<u64> {
        if is_local_model(&r.model) {
            return Some(0);
        }
        let p = self.find(&r.model)?;
        // USD por millón de tokens = micro-dólares por token.
        let micros = r.input as f64 * p.input
            + r.output as f64 * p.output
            + r.cache_read as f64 * p.cache_read
            + r.cache_write as f64 * p.cache_write;
        Some(micros.round() as u64)
    }

    /// Pone el costo estimado a los registros sin costo; el real (de la fuente) no se toca.
    pub fn apply(&self, records: &mut [UsageRecord]) {
        for r in records {
            if !matches!(r.cost, Cost::Real(_)) {
                r.cost = self.estimate(r).map_or(Cost::Unknown, Cost::Estimated);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentKind;

    fn rec(model: &str, input: u64, output: u64, cache_read: u64, cache_write: u64) -> UsageRecord {
        UsageRecord {
            agent: AgentKind::ClaudeCode,
            model: model.into(),
            ts: 0,
            input,
            output,
            cache_read,
            cache_write,
            session_id: "s".into(),
            title: None,
            project: None,
            cost: Cost::Unknown,
            key: None,
        }
    }

    const TABLA: &str = r#"
        [[precio]]
        modelo = "Demo-Grande"
        entrada = 3.0
        salida = 15.0
        cache_lectura = 0.30
        cache_escritura = 3.75

        [[precio]]
        modelo = "gpt-x"
        entrada = 1.0
        salida = 10.0
    "#;

    #[test]
    fn la_tabla_incluida_se_lee_y_trae_los_modelos_verificados() {
        let t = PriceTable::builtin();
        assert!(t.len() >= 15);
        assert!(t.find("claude-sonnet-5-5").is_some());
        assert!(t.find("claude-haiku-4-5-20251001").is_some());
        assert!(t.find("gpt-5.5").is_some());
        // Sin precio publicado: no se inventa.
        assert!(t.find("kimi-k3").is_none());
        assert!(t.find("glm-5.3-flash-free").is_none());
    }

    #[test]
    fn coincide_exacto_con_fecha_o_con_proveedor_y_nada_mas() {
        let t = PriceTable::parse(TABLA).unwrap();
        assert_eq!(t.find("demo-grande").map(|p| p.input), Some(3.0));
        assert!(t.find("DEMO-GRANDE").is_some());
        assert!(t.find("demo-grande-20251001").is_some());
        assert!(t.find("anthropic/demo-grande").is_some());
        // Otro modelo que solo comparte el comienzo del nombre no hereda el precio.
        assert!(t.find("gpt-x-codex").is_none());
        assert!(t.find("gpt-x.5").is_none());
        assert!(t.find("demo-grande-free").is_none());
    }

    #[test]
    fn modelos_locales_cuestan_cero_y_no_quedan_sin_precio() {
        let t = PriceTable::parse(TABLA).unwrap();
        for m in [
            "lmstudio/qwen2.5-coder-7b",
            "ollama/llama3.1:8b",
            "LM-Studio/demo",
            "llama.cpp/mi-modelo",
        ] {
            assert!(is_local_model(m), "{m} es local");
            assert_eq!(t.estimate(&rec(m, 1_000_000, 500_000, 0, 0)), Some(0), "{m} cuesta 0");
        }
        assert!(is_local_model("http://localhost:1234/v1/qwen"));
        // Un nombre parecido en la nube no es local.
        assert!(!is_local_model("qwen-max"));
        assert!(!is_local_model("anthropic/claude-sonnet-5-5"));
        assert!(!is_local_model("openrouter/qwen2.5-coder"));
        let mut records = vec![rec("ollama/llama3.1:8b", 10, 10, 0, 0)];
        t.apply(&mut records);
        assert_eq!(records[0].cost, Cost::Estimated(0));
    }

    #[test]
    fn costo_con_cache_en_micro_dolares() {
        let t = PriceTable::parse(TABLA).unwrap();
        // 1000 × 3 + 200 × 15 + 10 000 × 0,30 + 400 × 3,75 = 3000 + 3000 + 3000 + 1500.
        assert_eq!(t.estimate(&rec("demo-grande", 1_000, 200, 10_000, 400)), Some(10_500));
        // Sin precios de caché en la tabla: cuentan cero.
        assert_eq!(t.estimate(&rec("gpt-x", 1_000_000, 0, 5_000, 0)), Some(1_000_000));
        assert_eq!(t.estimate(&rec("sin-precio", 1, 1, 1, 1)), None);
    }

    #[test]
    fn aplicar_respeta_el_costo_real() {
        let t = PriceTable::parse(TABLA).unwrap();
        let mut records = vec![
            rec("demo-grande", 1_000, 0, 0, 0),
            rec("sin-precio", 1, 1, 1, 1),
            rec("demo-grande", 1_000, 0, 0, 0),
        ];
        records[2].cost = Cost::Real(42);
        t.apply(&mut records);
        assert_eq!(records[0].cost, Cost::Estimated(3_000));
        assert_eq!(records[1].cost, Cost::Unknown);
        assert_eq!(records[2].cost, Cost::Real(42));
    }

    #[test]
    fn las_entradas_del_usuario_ganan_y_lo_invalido_se_rechaza() {
        let user = PriceTable::parse("[[precio]]\nmodelo = \"gpt-x\"\nentrada = 2.0\nsalida = 20.0\n").unwrap();
        let t = PriceTable::parse(TABLA).unwrap().with_overrides(user);
        assert_eq!(t.find("gpt-x").map(|p| p.input), Some(2.0));
        assert_eq!(t.len(), 2);
        assert!(PriceTable::parse("[[precio]]\nmodelo = \"a\"\nentrada = -1.0\nsalida = 1.0\n").is_err());
        assert!(PriceTable::parse("no es toml [").is_err());
    }
}
