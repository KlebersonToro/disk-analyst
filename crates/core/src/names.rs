//! Buffer contínuo com todos os nomes em UTF-16 bruto (ADR-0006).
//!
//! Nomes do NTFS podem conter surrogates soltos; por isso o buffer guarda `u16` sem validar.
//! A conversão para texto só acontece na borda, em [`to_text`].

/// `Vec<u16>` contínuo com todos os nomes de uma varredura.
#[derive(Debug, Default, Clone)]
pub struct NameBuf {
    units: Vec<u16>,
}

impl NameBuf {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(units: usize) -> Self {
        Self {
            units: Vec::with_capacity(units),
        }
    }

    /// Acrescenta um nome e devolve `(offset, comprimento)` em unidades UTF-16.
    ///
    /// # Panics
    ///
    /// Se o nome tiver mais de `u16::MAX` unidades ou se o offset não couber em `u32`. Nunca
    /// trunca em silêncio: nomes reais têm no máximo 255 unidades e um buffer de 4 Gi unidades
    /// (8 GiB) está fora do orçamento de memória.
    pub fn push(&mut self, name: &[u16]) -> (u32, u16) {
        let len = u16::try_from(name.len())
            .unwrap_or_else(|_| panic!("nome longo demais: {} unidades UTF-16", name.len()));
        let off = u32::try_from(self.units.len())
            .unwrap_or_else(|_| panic!("NameBuf cheio: offset não cabe em u32"));
        self.units.extend_from_slice(name);
        (off, len)
    }

    /// Devolve o nome em `off..off + len`.
    ///
    /// # Panics
    ///
    /// Se o intervalo estiver fora do buffer (panic de slice padrão).
    pub fn get(&self, off: u32, len: u16) -> &[u16] {
        let start = off as usize;
        &self.units[start..start + usize::from(len)]
    }
}

/// Converte um nome UTF-16 em texto. Surrogates soltos viram U+FFFD e o `bool` devolvido
/// (`lossy`) fica `true`.
pub fn to_text(name: &[u16]) -> (String, bool) {
    let mut lossy = false;
    let text = char::decode_utf16(name.iter().copied())
        .map(|unit| {
            unit.unwrap_or_else(|_| {
                lossy = true;
                char::REPLACEMENT_CHARACTER
            })
        })
        .collect();
    (text, lossy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    proptest! {
        #[test]
        fn round_trip_of_several_pushes(names in proptest::collection::vec(".{0,64}", 0..16)) {
            let mut buf = NameBuf::new();
            let refs: Vec<(u32, u16)> = names.iter().map(|n| buf.push(&utf16(n))).collect();
            for (name, (off, len)) in names.iter().zip(refs) {
                let (text, lossy) = to_text(buf.get(off, len));
                prop_assert_eq!(&text, name);
                prop_assert!(!lossy);
            }
        }
    }

    #[test]
    fn lone_surrogate_becomes_replacement_char_and_lossy() {
        assert_eq!(to_text(&[0xD800, 0x0041]), ("\u{FFFD}A".to_owned(), true));
    }

    #[test]
    fn name_of_255_units_is_accepted() {
        let name = vec![0x0061u16; 255];
        let mut buf = NameBuf::new();
        let (off, len) = buf.push(&name);
        assert_eq!(len, 255);
        assert_eq!(buf.get(off, len), name.as_slice());
    }

    #[test]
    fn empty_name_has_zero_len_and_empty_text() {
        let mut buf = NameBuf::new();
        let (off, len) = buf.push(&[]);
        assert_eq!(len, 0);
        assert_eq!(to_text(buf.get(off, len)), (String::new(), false));
    }

    #[test]
    #[should_panic(expected = "nome longo demais")]
    fn name_longer_than_u16_max_panics() {
        let mut buf = NameBuf::new();
        buf.push(&vec![0x0061u16; usize::from(u16::MAX) + 1]);
    }
}
