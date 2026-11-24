//! Poner una imagen en el portapapeles para que un agente la lea con su propia tecla (ajustes v0.2:
//! arrastrar o elegir un archivo de imagen con Claude Code abierto).
//!
//! GPUI escribe las imágenes en Windows solo con el formato registrado "PNG"; PowerShell / .NET
//! (y con ellos Claude Code en Windows) leen `CF_DIB`. Por eso en Windows se escriben los dos. En
//! Linux y Mac basta el portapapeles de GPUI (PNG).

/// `CF_DIB` de Windows: BITMAPINFOHEADER (40 bytes) + píxeles BGRA de abajo hacia arriba, 32 bits.
pub fn dib_from_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let row = width as usize * 4;
    let mut out = Vec::with_capacity(40 + row * height as usize);
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(width as i32).to_le_bytes()); // biWidth
    out.extend_from_slice(&(height as i32).to_le_bytes()); // biHeight > 0: de abajo hacia arriba
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    out.extend_from_slice(&((row * height as usize) as u32).to_le_bytes()); // biSizeImage
    out.extend_from_slice(&2835i32.to_le_bytes()); // 72 ppp
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant
    for y in (0..height as usize).rev() {
        for px in rgba[y * row..(y + 1) * row].as_chunks::<4>().0 {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    out
}

/// Escribe el PNG en el portapapeles de Windows como `CF_DIB` y "PNG".
#[cfg(windows)]
pub fn write_png(png: &[u8]) -> Result<(), String> {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

    const CF_DIB: u32 = 8;
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let dib = dib_from_rgba(img.width(), img.height(), img.as_raw());
    let name: Vec<u16> = "PNG".encode_utf16().chain(std::iter::once(0)).collect();

    /// Copia `data` a memoria global y se la entrega al portapapeles (que pasa a ser su dueño).
    ///
    /// SAFETY: el portapapeles está abierto por este hilo.
    unsafe fn put(format: u32, data: &[u8]) -> bool {
        unsafe {
            let mem = GlobalAlloc(GMEM_MOVEABLE, data.len());
            if mem.is_null() {
                return false;
            }
            let dst = GlobalLock(mem) as *mut u8;
            if dst.is_null() {
                GlobalFree(mem);
                return false;
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
            GlobalUnlock(mem);
            if SetClipboardData(format, mem).is_null() {
                GlobalFree(mem);
                return false;
            }
            true
        }
    }

    // Otro programa puede tener el portapapeles abierto un instante: se reintenta.
    let mut opened = false;
    for _ in 0..10 {
        // SAFETY: sin ventana dueña (null); se cierra abajo.
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            opened = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if !opened {
        return Err("el portapapeles está ocupado".into());
    }
    // SAFETY: el portapapeles quedó abierto por este hilo; `name` termina en 0.
    let ok = unsafe {
        let ok = EmptyClipboard() != 0 && put(CF_DIB, &dib);
        let png_format = RegisterClipboardFormatW(name.as_ptr());
        if png_format != 0 {
            put(png_format, png);
        }
        CloseClipboard();
        ok
    };
    if ok {
        Ok(())
    } else {
        Err("no se pudo escribir la imagen en el portapapeles".into())
    }
}

#[cfg(test)]
mod tests {
    use super::dib_from_rgba;

    #[test]
    fn dib_de_abajo_hacia_arriba_en_bgra() {
        // 2×2: fila de arriba roja y verde, fila de abajo azul y blanca.
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 255, //
            0, 0, 255, 255, 255, 255, 255, 128,
        ];
        let dib = dib_from_rgba(2, 2, &rgba);
        assert_eq!(dib.len(), 40 + 16);
        assert_eq!(u32::from_le_bytes(dib[0..4].try_into().unwrap()), 40);
        assert_eq!(i32::from_le_bytes(dib[4..8].try_into().unwrap()), 2);
        assert_eq!(i32::from_le_bytes(dib[8..12].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(dib[14..16].try_into().unwrap()), 32);
        // Primero va la fila de abajo (azul, blanco), en BGRA.
        assert_eq!(&dib[40..44], &[255, 0, 0, 255]);
        assert_eq!(&dib[44..48], &[255, 255, 255, 128]);
        // Después la de arriba (rojo, verde).
        assert_eq!(&dib[48..52], &[0, 0, 255, 255]);
        assert_eq!(&dib[52..56], &[0, 255, 0, 255]);
    }
}
