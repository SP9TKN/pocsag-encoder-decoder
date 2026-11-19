//! Lista częstotliwości do szybkiego przestrojenia radia.
//!
//! Plik `czestotliwosci.txt` obok programu, UTF-8 + CRLF, linia na pozycję: `nazwa | MHz`
//! (MHz z przecinkiem albo kropką). Da się go poprawiać w Notatniku.

use crate::ptt::{czest_z_tekstu, tekst_czest};
use std::path::PathBuf;

pub const NAGLOWEK: &str = "nazwa | MHz";
/// Ile ostatnio użytych częstotliwości pamięta program.
pub const OSTATNICH: usize = 8;

#[derive(Clone, PartialEq, Debug)]
pub struct Czest {
    pub nazwa: String,
    pub hz: u64,
}

pub fn sciezka() -> std::io::Result<PathBuf> {
    let mut p = std::env::current_exe()?;
    p.set_file_name("czestotliwosci.txt");
    Ok(p)
}

/// Pierwsza lista, zanim powstanie plik: częstotliwości używane dotąd z programem.
pub fn startowe() -> Vec<Czest> {
    vec![Czest { nazwa: "70 cm".into(), hz: 435_000_000 }, Czest { nazwa: "2 m".into(), hz: 144_900_000 }]
}

pub fn linia(c: &Czest) -> String {
    format!("{} | {}", c.nazwa.replace('|', "¦").replace(['\r', '\n'], " ").trim(), tekst_czest(c.hz))
}

pub fn odczytaj_linie(l: &str) -> Option<Czest> {
    let (nazwa, mhz) = l.split_once('|')?;
    Some(Czest { nazwa: nazwa.trim().to_string(), hz: czest_z_tekstu(mhz).ok()? })
}

pub fn wczytaj() -> Vec<Czest> {
    match sciezka().and_then(std::fs::read_to_string) {
        Ok(t) => t.lines().filter_map(odczytaj_linie).collect(),
        Err(_) => startowe(),
    }
}

pub fn zapisz(lista: &[Czest]) -> std::io::Result<()> {
    let mut t = format!("{NAGLOWEK}\r\n");
    for c in lista {
        t += &linia(c);
        t += "\r\n";
    }
    std::fs::write(sciezka()?, t)
}

/// Ostatnio użyte: ta na początek, bez powtórek, najwyżej `OSTATNICH`.
pub fn dopisz_ostatnia(ostatnie: &mut Vec<u64>, hz: u64) {
    ostatnie.retain(|&x| x != hz);
    ostatnie.insert(0, hz);
    ostatnie.truncate(OSTATNICH);
}

#[cfg(test)]
mod testy {
    use super::*;

    #[test]
    fn linia_tam_i_z_powrotem() {
        let c = Czest { nazwa: "Pager | zakład".into(), hz: 446_156_250 };
        let l = linia(&c);
        assert_eq!(l, "Pager ¦ zakład | 446,15625");
        assert_eq!(odczytaj_linie(&l), Some(Czest { nazwa: "Pager ¦ zakład".into(), hz: 446_156_250 }));
        assert_eq!(odczytaj_linie("2 m | 144.9"), Some(Czest { nazwa: "2 m".into(), hz: 144_900_000 }));
        assert!(odczytaj_linie(NAGLOWEK).is_none(), "nagłówek to nie częstotliwość");
        assert!(odczytaj_linie("zła | abc").is_none());
    }

    #[test]
    fn ostatnie_bez_powtorek_i_z_limitem() {
        let mut o = vec![];
        for hz in [1_000_000u64, 2_000_000, 1_000_000] {
            dopisz_ostatnia(&mut o, hz);
        }
        assert_eq!(o, vec![1_000_000, 2_000_000]);
        for i in 0..20u64 {
            dopisz_ostatnia(&mut o, 10_000_000 + i);
        }
        assert_eq!(o.len(), OSTATNICH);
        assert_eq!(o[0], 10_000_019);
    }
}
