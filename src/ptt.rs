//! Kluczowanie nadajnika: linia DTR, RTS albo obie naraz portu COM, albo polecenie CAT (Icom CI-V, Yaesu).
//!
//! Port jest otwierany raz, przy starcie, i trzymany do końca: każde otwarcie portu może
//! na chwilę podnieść linie (sterownik bierze stary stan), więc nie otwieramy go przy każdym nadaniu.

use serialport::{ClearBuffer, FlowControl, SerialPort};
use std::io::Write;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RodzajPtt {
    /// Nadajnik sam z VOX albo kluczowany inaczej.
    Brak,
    Dtr,
    Rts,
    /// Obie linie naraz: interfejsy kluczowane jedną albo drugą działają bez sprawdzania którą.
    DtrRts,
    CatIcom,
    CatYaesu,
}

impl RodzajPtt {
    pub const WSZYSTKIE: [RodzajPtt; 6] =
        [RodzajPtt::Brak, RodzajPtt::Dtr, RodzajPtt::Rts, RodzajPtt::DtrRts, RodzajPtt::CatIcom, RodzajPtt::CatYaesu];

    pub fn nazwa(self) -> &'static str {
        match self {
            RodzajPtt::Brak => "Brak (VOX)",
            RodzajPtt::Dtr => "COM: linia DTR",
            RodzajPtt::Rts => "COM: linia RTS",
            RodzajPtt::DtrRts => "COM: linie DTR+RTS",
            RodzajPtt::CatIcom => "CAT Icom (CI-V)",
            RodzajPtt::CatYaesu => "CAT Yaesu (TX1;/TX0;)",
        }
    }

    /// Nazwa w pliku ustawień.
    pub fn klucz(self) -> &'static str {
        match self {
            RodzajPtt::Brak => "brak",
            RodzajPtt::Dtr => "dtr",
            RodzajPtt::Rts => "rts",
            RodzajPtt::DtrRts => "dtr_rts",
            RodzajPtt::CatIcom => "icom",
            RodzajPtt::CatYaesu => "yaesu",
        }
    }

    pub fn z_klucza(k: &str) -> Option<Self> {
        Self::WSZYSTKIE.into_iter().find(|r| r.klucz() == k)
    }

    pub fn uzywa_portu(self) -> bool {
        self != RodzajPtt::Brak
    }

    pub fn cat(self) -> bool {
        matches!(self, RodzajPtt::CatIcom | RodzajPtt::CatYaesu)
    }

    /// Kluczowanie liniami portu (DTR, RTS albo obie).
    pub fn linia(self) -> bool {
        matches!(self, RodzajPtt::Dtr | RodzajPtt::Rts | RodzajPtt::DtrRts)
    }
}

pub const PREDKOSCI_CAT: [u32; 6] = [4800, 9600, 19200, 38400, 57600, 115200];

#[derive(Clone, PartialEq, Debug)]
pub struct UstawieniaPtt {
    pub rodzaj: RodzajPtt,
    pub port: String,
    /// Prędkość portu dla CAT; przy DTR/RTS bez znaczenia.
    pub predkosc: u32,
    /// Adres radia w CI-V (IC-7100: 0x88).
    pub adres_civ: u8,
    /// Linia aktywna stanem niskim (np. tranzystor kluczujący odwrotnie).
    pub odwroc: bool,
}

impl UstawieniaPtt {
    /// Krótki opis do historii, np. „COM20 DTR”.
    pub fn opis(&self) -> String {
        match self.rodzaj {
            RodzajPtt::Brak => "VOX".into(),
            RodzajPtt::Dtr | RodzajPtt::Rts | RodzajPtt::DtrRts => format!(
                "{} {}{}",
                self.port,
                match self.rodzaj {
                    RodzajPtt::Dtr => "DTR",
                    RodzajPtt::Rts => "RTS",
                    _ => "DTR+RTS",
                },
                if self.odwroc { " odwr." } else { "" }
            ),
            RodzajPtt::CatIcom => format!("{} CI-V {:02X}h", self.port, self.adres_civ),
            RodzajPtt::CatYaesu => format!("{} CAT Yaesu", self.port),
        }
    }
}

/// Sterowanie radiem przez CAT (częstotliwość). Osobno od PTT: FT-991A przyjmuje CAT tylko na
/// porcie „Enhanced”, a PTT linią RTS idzie przez port „Standard”.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RodzajCat {
    Brak,
    Icom,
    Yaesu,
}

impl RodzajCat {
    pub const WSZYSTKIE: [RodzajCat; 3] = [RodzajCat::Brak, RodzajCat::Icom, RodzajCat::Yaesu];

    pub fn nazwa(self) -> &'static str {
        match self {
            RodzajCat::Brak => "wyłączone",
            RodzajCat::Icom => "Icom CI-V",
            RodzajCat::Yaesu => "Yaesu CAT (FA)",
        }
    }

    pub fn klucz(self) -> &'static str {
        match self {
            RodzajCat::Brak => "brak",
            RodzajCat::Icom => "icom",
            RodzajCat::Yaesu => "yaesu",
        }
    }

    pub fn z_klucza(k: &str) -> Option<Self> {
        Self::WSZYSTKIE.into_iter().find(|r| r.klucz() == k)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct UstawieniaCat {
    pub rodzaj: RodzajCat,
    pub port: String,
    pub predkosc: u32,
    /// Adres radia w CI-V (IC-7100: 0x88).
    pub adres_civ: u8,
}

/// Częstotliwość w Hz z tekstu w MHz: „435,000”, „435.000”, „144.9”, „446.15625 MHz”.
pub fn czest_z_tekstu(t: &str) -> Result<u64, String> {
    let s = t.trim().trim_end_matches(|c: char| c.is_ascii_alphabetic() || c.is_whitespace()).replace(',', ".");
    let mhz: f64 = s.parse().map_err(|_| format!("„{}” to nie częstotliwość w MHz (np. 435,000)", t.trim()))?;
    if !(0.03..=1300.0).contains(&mhz) {
        return Err(format!("{} MHz: poza zakresem radia", t.trim()));
    }
    Ok((mhz * 1e6).round() as u64)
}

/// MHz z przecinkiem i co najmniej 3 miejscami: 435000000 -> „435,000”, 446156250 -> „446,15625”.
pub fn tekst_czest(hz: u64) -> String {
    let mut ul = format!("{:06}", hz % 1_000_000);
    while ul.len() > 3 && ul.ends_with('0') {
        ul.pop();
    }
    format!("{},{ul}", hz / 1_000_000)
}

/// Częstotliwość w BCD dla CI-V: 5 bajtów, najmłodsze cyfry pierwsze.
pub fn czest_bcd(hz: u64) -> [u8; 5] {
    let mut b = [0u8; 5];
    let mut v = hz;
    for x in b.iter_mut() {
        let lo = v % 10;
        v /= 10;
        let hi = v % 10;
        v /= 10;
        *x = (hi << 4 | lo) as u8;
    }
    b
}

/// Polecenie ustawienia częstotliwości: CI-V 05 albo Yaesu „FA”.
pub fn ramka_czestotliwosci(cat: &UstawieniaCat, hz: u64) -> Vec<u8> {
    match cat.rodzaj {
        RodzajCat::Icom => {
            let mut r = vec![0xFE, 0xFE, cat.adres_civ, 0xE0, 0x05];
            r.extend_from_slice(&czest_bcd(hz));
            r.push(0xFD);
            r
        }
        RodzajCat::Yaesu => format!("FA{hz:09};").into_bytes(),
        RodzajCat::Brak => Vec::new(),
    }
}

/// Odpowiedź radia w strumieniu CI-V: FB (przyjęte) albo FA (odrzucone) w ramce
/// FE FE E0 <radio> .. FD; echo własnej ramki (FE FE <radio> E0 ..) jest pomijane.
fn odpowiedz_civ(b: &[u8], adres: u8) -> Option<u8> {
    b.windows(6)
        .find(|w| w[0] == 0xFE && w[1] == 0xFE && w[2] == 0xE0 && w[3] == adres && w[5] == 0xFD && matches!(w[4], 0xFB | 0xFA))
        .map(|w| w[4])
}

fn czytaj(p: &mut dyn SerialPort, ms: u64, koniec: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let start = std::time::Instant::now();
    let (mut buf, mut tmp) = (Vec::new(), [0u8; 64]);
    while start.elapsed() < Duration::from_millis(ms) {
        match p.bytes_to_read() {
            Ok(n) if n > 0 => {
                if let Ok(k) = std::io::Read::read(p, &mut tmp) {
                    buf.extend_from_slice(&tmp[..k]);
                    if koniec(&buf) {
                        break;
                    }
                }
            }
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    buf
}

/// Ustawia częstotliwość na otwartym porcie i sprawdza, czy radio ją przyjęło.
fn ustaw_na_porcie(p: &mut dyn SerialPort, cat: &UstawieniaCat, hz: u64) -> Result<(), String> {
    if cat.rodzaj == RodzajCat::Brak {
        return Err("sterowanie radiem wyłączone w ustawieniach".into());
    }
    let _ = p.clear(ClearBuffer::Input);
    p.write_all(&ramka_czestotliwosci(cat, hz)).and_then(|_| p.flush()).map_err(|e| format!("CAT {}: {e}", cat.port))?;
    if cat.rodzaj == RodzajCat::Icom {
        let odp = czytaj(p, 700, |b| odpowiedz_civ(b, cat.adres_civ).is_some());
        return match odpowiedz_civ(&odp, cat.adres_civ) {
            Some(0xFB) => Ok(()),
            Some(_) => Err("radio odrzuciło częstotliwość (poza pasmem albo kanał z pamięci?)".into()),
            None => Err(format!("radio nie odpowiada na CI-V ({}, adres {:02X}h) — włączone? prędkość?", cat.port, cat.adres_civ)),
        };
    }
    // Yaesu nie potwierdza zapisu: odczyt FA; pokazuje, co radio ma naprawdę.
    std::thread::sleep(Duration::from_millis(150));
    let _ = p.clear(ClearBuffer::Input);
    p.write_all(b"FA;").and_then(|_| p.flush()).map_err(|e| format!("CAT {}: {e}", cat.port))?;
    let odp = String::from_utf8_lossy(&czytaj(p, 700, |b| b.ends_with(b";"))).to_string();
    let podane = odp.strip_prefix("FA").and_then(|s| s.strip_suffix(';')).and_then(|s| s.parse::<u64>().ok());
    match podane {
        Some(x) if x == hz => Ok(()),
        Some(x) => Err(format!("radio zostało na {} MHz (poza pasmem albo kanał z pamięci?)", tekst_czest(x))),
        None if odp.is_empty() => Err(format!("radio nie odpowiada na CAT ({}) — włączone? prędkość?", cat.port)),
        None => Err(format!("nieczytelna odpowiedź radia ({}) — zła prędkość CAT?", cat.port)),
    }
}

/// Port „Standard” mostka CP2105 (FT-991A, FTDX10) to port PTT/klucza: RTS na nim nadaje.
fn port_standard(nazwa: &str) -> bool {
    porty().iter().any(|(n, e)| n == nazwa && e.contains("Standard COM Port"))
}

/// Ustawia częstotliwość, otwierając port CAT tylko na tę chwilę (inne programy mogą go używać).
pub fn czestotliwosc_osobno(cat: &UstawieniaCat, hz: u64) -> Result<(), String> {
    if cat.rodzaj == RodzajCat::Brak {
        return Err("sterowanie radiem wyłączone w ustawieniach".into());
    }
    if cat.port.is_empty() {
        return Err("sterowanie radiem: wybierz port COM".into());
    }
    if cat.rodzaj == RodzajCat::Yaesu && port_standard(&cat.port) {
        return Err(format!("{} to port Standard (PTT) — CAT Yaesu idzie przez port Enhanced", cat.port));
    }
    let mut p = serialport::new(&cat.port, cat.predkosc)
        .flow_control(FlowControl::None)
        .stop_bits(serialport::StopBits::Two)
        .dtr_on_open(false)
        .timeout(Duration::from_millis(300))
        .open()
        .map_err(|e| format!("CAT {}: {e}", cat.port))?;
    // Yaesu z włączonym „CAT RTS” odpowiada tylko przy RTS w górze (to linia portu CAT, nie PTT).
    let _ = p.write_data_terminal_ready(false);
    let _ = p.write_request_to_send(cat.rodzaj == RodzajCat::Yaesu);
    if cat.rodzaj == RodzajCat::Yaesu {
        std::thread::sleep(Duration::from_millis(100));
    }
    ustaw_na_porcie(p.as_mut(), cat, hz)
}

pub struct Ptt {
    port: Option<Box<dyn SerialPort>>,
    ust: UstawieniaPtt,
    cat: UstawieniaCat,
}

impl Ptt {
    pub fn otworz(ust: &UstawieniaPtt, cat: &UstawieniaCat) -> Result<Self, String> {
        if !ust.rodzaj.uzywa_portu() {
            return Ok(Self { port: None, ust: ust.clone(), cat: cat.clone() });
        }
        if ust.port.is_empty() {
            return Err("PTT: wybierz port COM".into());
        }
        // Port wspólny z CAT radia (IC-7100: CI-V do PTT i częstotliwości): prędkość CAT, gdy PTT jej nie ma.
        let predkosc = if cat.rodzaj != RodzajCat::Brak && cat.port == ust.port && !ust.rodzaj.cat() { cat.predkosc } else { ust.predkosc };
        let port = serialport::new(&ust.port, predkosc)
            .flow_control(FlowControl::None)
            .dtr_on_open(false)
            .timeout(Duration::from_millis(300))
            .open()
            .map_err(|e| format!("PTT {}: {e}", ust.port))?;
        let mut ptt = Self { port: Some(port), ust: ust.clone(), cat: cat.clone() };
        // Obie linie w spoczynek od razu po otwarciu, także ta nieużywana.
        ptt.linie(false)?;
        Ok(ptt)
    }

    /// Stan spoczynku obu linii; przy kluczowaniu liniami tylko używane dostają `nadaj`.
    fn linie(&mut self, nadaj: bool) -> Result<(), String> {
        let ust = &self.ust;
        let Some(p) = self.port.as_mut() else { return Ok(()) };
        let dtr = match ust.rodzaj {
            RodzajPtt::Dtr | RodzajPtt::DtrRts => nadaj != ust.odwroc,
            _ => false,
        };
        let rts = match ust.rodzaj {
            RodzajPtt::Rts | RodzajPtt::DtrRts => nadaj != ust.odwroc,
            _ => false,
        };
        p.write_data_terminal_ready(dtr).map_err(|e| format!("PTT DTR: {e}"))?;
        p.write_request_to_send(rts).map_err(|e| format!("PTT RTS: {e}"))
    }

    pub fn ustaw(&mut self, nadaj: bool) -> Result<(), String> {
        match self.ust.rodzaj {
            RodzajPtt::Brak => Ok(()),
            RodzajPtt::Dtr | RodzajPtt::Rts | RodzajPtt::DtrRts => self.linie(nadaj),
            RodzajPtt::CatIcom | RodzajPtt::CatYaesu => {
                let ramka: Vec<u8> = if self.ust.rodzaj == RodzajPtt::CatIcom {
                    // FE FE <radio> <komputer E0> 1C 00 <01 nadawanie / 00 odbiór> FD
                    vec![0xFE, 0xFE, self.ust.adres_civ, 0xE0, 0x1C, 0x00, nadaj as u8, 0xFD]
                } else if nadaj {
                    b"TX1;".to_vec()
                } else {
                    b"TX0;".to_vec()
                };
                let p = self.port.as_mut().ok_or("PTT: port zamknięty")?;
                // Echo i odpowiedzi radia nie są potrzebne: tylko czyścimy bufor wejściowy.
                let _ = p.clear(ClearBuffer::Input);
                p.write_all(&ramka).and_then(|_| p.flush()).map_err(|e| format!("PTT CAT: {e}"))
            }
        }
    }

    /// Częstotliwość radia: przez otwarty port PTT, gdy CAT idzie tym samym portem (IC-7100),
    /// inaczej przez port CAT otwierany na chwilę (FT-991A: CAT na COM4, PTT RTS na COM5).
    pub fn czestotliwosc(&mut self, hz: u64) -> Result<(), String> {
        match self.port.as_mut() {
            Some(p) if self.cat.rodzaj != RodzajCat::Brak && self.cat.port == self.ust.port => {
                ustaw_na_porcie(p.as_mut(), &self.cat, hz)
            }
            _ => czestotliwosc_osobno(&self.cat, hz),
        }
    }
}

impl Drop for Ptt {
    /// Zawsze zwalnia nadajnik przed zamknięciem portu.
    fn drop(&mut self) {
        let _ = self.ustaw(false);
    }
}

/// Porty COM z opisem do listy, np. („COM20”, „COM20 — com0com”).
pub fn porty() -> Vec<(String, String)> {
    let mut l: Vec<(String, String)> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let opis = match &p.port_type {
                serialport::SerialPortType::UsbPort(u) => {
                    u.product.clone().or_else(|| u.manufacturer.clone()).unwrap_or_else(|| "USB".into())
                }
                serialport::SerialPortType::BluetoothPort => "Bluetooth".into(),
                serialport::SerialPortType::PciPort => "PCI".into(),
                serialport::SerialPortType::Unknown => String::new(),
            };
            // Opis z Windows często kończy się „(COM6)”: bez powtórzenia nazwy.
            let opis = opis.trim_end_matches(&format!("({})", p.port_name)).trim_end().to_string();
            let etykieta = if opis.is_empty() { p.port_name.clone() } else { format!("{} — {opis}", p.port_name) };
            (p.port_name, etykieta)
        })
        .collect();
    // COM2 przed COM10.
    l.sort_by_key(|(n, _)| (n.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().unwrap_or(u32::MAX), n.clone()));
    l.dedup_by(|a, b| a.0 == b.0);
    l
}

#[cfg(test)]
mod testy {
    use super::*;

    fn cat(rodzaj: RodzajCat) -> UstawieniaCat {
        UstawieniaCat { rodzaj, port: "COM4".into(), predkosc: 38400, adres_civ: 0x88 }
    }

    #[test]
    fn czestotliwosc_z_tekstu_i_z_powrotem() {
        assert_eq!(czest_z_tekstu("435,000"), Ok(435_000_000));
        assert_eq!(czest_z_tekstu("435.000"), Ok(435_000_000));
        assert_eq!(czest_z_tekstu(" 144.9 "), Ok(144_900_000));
        assert_eq!(czest_z_tekstu("446,15625 MHz"), Ok(446_156_250));
        assert!(czest_z_tekstu("abc").is_err());
        assert!(czest_z_tekstu("0").is_err());
        assert!(czest_z_tekstu("5000").is_err());
        assert_eq!(tekst_czest(435_000_000), "435,000");
        assert_eq!(tekst_czest(144_900_000), "144,900");
        assert_eq!(tekst_czest(446_156_250), "446,15625");
        assert_eq!(tekst_czest(7_074_000), "7,074");
    }

    /// CI-V: 5 bajtów BCD, najmłodsze cyfry pierwsze (IC-7100, polecenie 05).
    #[test]
    fn ramki_icom_i_yaesu() {
        assert_eq!(czest_bcd(435_000_000), [0x00, 0x00, 0x00, 0x35, 0x04]);
        assert_eq!(czest_bcd(144_900_000), [0x00, 0x00, 0x90, 0x44, 0x01]);
        assert_eq!(czest_bcd(446_156_250), [0x50, 0x62, 0x15, 0x46, 0x04]);
        assert_eq!(
            ramka_czestotliwosci(&cat(RodzajCat::Icom), 435_000_000),
            vec![0xFE, 0xFE, 0x88, 0xE0, 0x05, 0x00, 0x00, 0x00, 0x35, 0x04, 0xFD]
        );
        assert_eq!(ramka_czestotliwosci(&cat(RodzajCat::Yaesu), 144_900_000), b"FA144900000;".to_vec());
        assert!(ramka_czestotliwosci(&cat(RodzajCat::Brak), 1).is_empty());
    }

    /// Echo własnej ramki (FE FE 88 E0 ..) nie jest odpowiedzią; liczy się FE FE E0 88 FB/FA FD.
    #[test]
    fn odpowiedz_civ_pomija_echo() {
        let mut b = ramka_czestotliwosci(&cat(RodzajCat::Icom), 435_000_000);
        assert_eq!(odpowiedz_civ(&b, 0x88), None);
        b.extend_from_slice(&[0xFE, 0xFE, 0xE0, 0x88, 0xFB, 0xFD]);
        assert_eq!(odpowiedz_civ(&b, 0x88), Some(0xFB));
        assert_eq!(odpowiedz_civ(&[0xFE, 0xFE, 0xE0, 0x88, 0xFA, 0xFD], 0x88), Some(0xFA));
        assert_eq!(odpowiedz_civ(&[0xFE, 0xFE, 0xE0, 0x94, 0xFB, 0xFD], 0x88), None, "inne radio");
    }
}
