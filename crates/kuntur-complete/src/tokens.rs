//! Partir la línea escrita en palabras, como PowerShell: espacios fuera de comillas separan
//! palabras; `;`, `|` y `&` empiezan otro comando; el acento grave escapa el carácter siguiente.

/// Una palabra tal como se escribió (con sus comillas, si las tiene).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub text: String,
}

/// Palabras del **último** comando de la línea.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub tokens: Vec<Token>,
    /// La línea termina en un espacio (fuera de comillas): se empieza una palabra nueva.
    pub ends_with_space: bool,
}

pub fn parse(input: &str) -> Line {
    let mut line = Line::default();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();
    let flush = |cur: &mut String, line: &mut Line| {
        if !cur.is_empty() {
            line.tokens.push(Token {
                text: std::mem::take(cur),
            });
        }
    };
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => {
                quote = None;
                cur.push(c);
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                cur.push(c);
            }
            (None, '`') => {
                cur.push(c);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (None, c) if c.is_whitespace() => flush(&mut cur, &mut line),
            (None, ';' | '|' | '&') => {
                cur.clear();
                line.tokens.clear();
            }
            (None, c) => cur.push(c),
        }
    }
    line.ends_with_space = quote.is_none() && cur.is_empty() && input.ends_with(char::is_whitespace);
    flush(&mut cur, &mut line);
    line
}

/// ¿Parece una ruta? (`.\x`, `..`, `~\x`, `C:\x`, `\\servidor`, `a\b`).
pub fn looks_like_path(word: &str) -> bool {
    let w = word.trim_start_matches(['\'', '"']);
    w.starts_with('.') || w.starts_with('~') || w.contains(['\\', '/']) || {
        let b = w.as_bytes();
        b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(input: &str) -> Vec<String> {
        parse(input).tokens.into_iter().map(|t| t.text).collect()
    }

    #[test]
    fn separa_por_espacios_y_respeta_comillas() {
        assert_eq!(words("cd 'C:\\Program Files'"), vec!["cd", "'C:\\Program Files'"]);
        assert_eq!(words("cd \"C:\\Program Fi"), vec!["cd", "\"C:\\Program Fi"]);
        assert_eq!(words("git  commit   -m"), vec!["git", "commit", "-m"]);
        assert_eq!(words("dir a` b"), vec!["dir", "a` b"]);
    }

    #[test]
    fn solo_el_ultimo_comando() {
        assert_eq!(words("dir; cd C:\\Win"), vec!["cd", "C:\\Win"]);
        assert_eq!(
            words("Get-ChildItem | Select-Object -First"),
            vec!["Select-Object", "-First"]
        );
        assert_eq!(words("echo 'a;b'"), vec!["echo", "'a;b'"]);
    }

    #[test]
    fn espacio_final() {
        assert!(parse("cd ").ends_with_space);
        assert!(!parse("cd").ends_with_space);
        assert!(!parse("cd 'a ").ends_with_space, "dentro de comillas no cuenta");
    }

    #[test]
    fn pinta_de_ruta() {
        for w in [
            ".\\scr", "..", "~\\Doc", "C:\\Win", "c:", "src/lib", "'C:\\Pro", "\\\\srv",
        ] {
            assert!(looks_like_path(w), "{w}");
        }
        for w in ["git", "Get-ChildItem", "-Force"] {
            assert!(!looks_like_path(w), "{w}");
        }
    }
}
