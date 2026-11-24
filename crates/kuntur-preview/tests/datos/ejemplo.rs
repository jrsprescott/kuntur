//! Código de ejemplo (sintético) para la vista previa de texto con números de línea.

/// Suma los números pares de una lista.
fn suma_pares(numeros: &[i32]) -> i32 {
    numeros.iter().filter(|n| *n % 2 == 0).sum()
}

fn main() {
    let datos = [1, 2, 3, 4, 5, 6];
    println!("Suma de pares: {}", suma_pares(&datos));
}
