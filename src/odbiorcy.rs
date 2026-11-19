//! Książka odbiorców: nazwa pagera i to, czego potrzeba, żeby do niego nadać.
//!
//! Plik `odbiorcy.txt` obok programu, UTF-8 + CRLF, linia na odbiorcę:
//! `nazwa | RIC | funkcja | typ | prędkość`. Da się go poprawiać w Notatniku.

use crate::koder::{MAX_RIC, Rodzaj};
use crate::pocsag::PREDKOSCI;
use std::path::PathBuf;

pub const NAGLOWEK: &str = "nazwa | RIC | funkcja | typ | prędkość";

#[derive(Clone, PartialEq, Debug)]
pub struct Odbiorca {
    pub nazwa: String,
    pub ric: u32,
    pub funkcja: u8,
    pub rodzaj: Rodzaj,
    /// Pager odbiera tylko na swojej prędkości.
    pub predkosc: u32,
}

pub fn sciezka() -> std::io::Result<PathBuf> {
    let mut p = std::env::current_exe()?;
    p.set_file_name("odbiorcy.txt");
    Ok(p)
}

pub fn linia(o: &Odbiorca) -> String {
    format!(
        "{} | {} | {} | {} | {}",
        o.nazwa.replace('|', "¦").replace(['\r', '\n'], " ").trim(),
        o.ric,
        o.funkcja,
        o.rodzaj.nazwa(),
        o.predkosc
    )
}

pub fn odczytaj_linie(l: &str) -> Option<Odbiorca> {
    let p: Vec<&str> = l.splitn(5, '|').map(str::trim).collect();
    if p.len() != 5 {
        return None;
    }
    Some(Odbiorca {
        nazwa: p[0].to_string(),
        ric: p[1].parse().ok().filter(|r| *r <= MAX_RIC)?,
        funkcja: p[2].parse().ok().filter(|f| *f <= 3)?,
        rodzaj: Rodzaj::WSZYSTKIE.into_iter().find(|r| r.nazwa() == p[3])?,
        predkosc: p[4].parse().ok().filter(|v| PREDKOSCI.contains(v))?,
    })
}

pub fn wczytaj() -> Vec<Odbiorca> {
    let Ok(s) = sciezka() else { return Vec::new() };
    std::fs::read_to_string(s).map(|t| t.lines().filter_map(odczytaj_linie).collect()).unwrap_or_default()
}

pub fn zapisz(lista: &[Odbiorca]) -> std::io::Result<()> {
    let mut t = format!("{NAGLOWEK}\r\n");
    for o in lista {
        t += &linia(o);
        t += "\r\n";
    }
    std::fs::write(sciezka()?, t)
}

/// Odbiorca o dokładnie tym RIC i tej funkcji (wybór w konsoli).
pub fn nazwa_dokladna(lista: &[Odbiorca], ric: u32, funkcja: u8) -> Option<&str> {
    lista.iter().find(|o| o.ric == ric && o.funkcja == funkcja).map(|o| o.nazwa.as_str())
}

/// Nazwa odbiorcy dla RIC: najpierw z tą samą funkcją, potem z samym RIC.
pub fn nazwa(lista: &[Odbiorca], ric: u32, funkcja: u8) -> Option<&str> {
    lista
        .iter()
        .find(|o| o.ric == ric && o.funkcja == funkcja)
        .or_else(|| lista.iter().find(|o| o.ric == ric))
        .map(|o| o.nazwa.as_str())
}

#[cfg(test)]
mod testy {
    use super::*;

    #[test]
    fn linia_tam_i_z_powrotem() {
        let o = Odbiorca { nazwa: "Jan | magazyn".into(), ric: 1234560, funkcja: 3, rodzaj: Rodzaj::Sygnal, predkosc: 512 };
        let l = linia(&o);
        assert_eq!(l, "Jan ¦ magazyn | 1234560 | 3 | sam sygnał | 512");
        assert_eq!(odczytaj_linie(&l), Some(Odbiorca { nazwa: "Jan ¦ magazyn".into(), ..o.clone() }));
        assert!(odczytaj_linie(NAGLOWEK).is_none());
        assert!(odczytaj_linie("x | 1 | 7 | tekst | 512").is_none(), "funkcja tylko 0-3");
        assert!(odczytaj_linie("x | 1 | 0 | tekst | 300").is_none(), "prędkość z listy");
        let lista = vec![o.clone(), Odbiorca { nazwa: "Grupa".into(), funkcja: 0, ..o }];
        assert_eq!(nazwa(&lista, 1234560, 0), Some("Grupa"));
        assert_eq!(nazwa(&lista, 1234560, 2), Some("Jan | magazyn"));
        assert_eq!(nazwa(&lista, 5, 0), None);
    }
}
