#![windows_subsystem = "windows"]

use eframe::egui;
use egui_extras::{Column, TableBuilder, TableRow};
use pogsag_koder::dziennik::{self, Wpis};
use pogsag_koder::odbiorcy::{self, Odbiorca};
use pogsag_koder::koder::{self, DoNadania, MAX_RIC, MAX_ZNAKOW, Rodzaj};
use pogsag_koder::korekcja::Korekcja;
use pogsag_koder::czestotliwosci::{self, Czest};
use pogsag_koder::ptt::{self, PREDKOSCI_CAT, RodzajCat, RodzajPtt, UstawieniaCat, UstawieniaPtt};
use pogsag_koder::silnik::{self, BEZ_NASLUCHU, Parametry, Silnik, Wspolne, Zlecenie};
use pogsag_koder::{plik, pocsag};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const ZIELONY: egui::Color32 = egui::Color32::from_rgb(70, 190, 110);
const ZOLTY: egui::Color32 = egui::Color32::from_rgb(225, 180, 70);
const CZERWONY: egui::Color32 = egui::Color32::from_rgb(230, 90, 80);
const TON_PROBNY_S: f32 = 3.0;

// Tabela historii: jasna, jak arkusz kalkulacyjny.
const TAB_TEKST: egui::Color32 = egui::Color32::from_gray(25);
const TAB_SIATKA: egui::Color32 = egui::Color32::from_gray(205);
const TAB_NAGLOWEK: egui::Color32 = egui::Color32::from_gray(232);
const TAB_PASEK: egui::Color32 = egui::Color32::from_gray(246);
const TAB_NAJAZD: egui::Color32 = egui::Color32::from_rgb(221, 235, 252);
const TAB_NADANE: egui::Color32 = egui::Color32::from_rgb(20, 80, 170);
const TAB_ODEBRANE: egui::Color32 = egui::Color32::from_rgb(0, 120, 50);
const TAB_UWAGA: egui::Color32 = egui::Color32::from_rgb(185, 85, 0);
const TAB_BLAD: egui::Color32 = egui::Color32::from_rgb(190, 30, 30);
/// Podświetlenie wiadomości do programu: na własny RIC i na RIC grupowy.
const TAB_DO_MNIE: egui::Color32 = egui::Color32::from_rgb(255, 238, 186);
const TAB_DO_GRUPY: egui::Color32 = egui::Color32::from_rgb(214, 238, 205);

/// Program jako pager: własny RIC i RIC grupowy, każdy z włącznikiem (numer zostaje,
/// gdy jest wyłączony).
#[derive(Clone, Copy, PartialEq, Debug)]
struct MojeRic {
    wlasny: u32,
    wlasny_wl: bool,
    /// Własny RIC na początku każdej wysłanej wiadomości: „(1234567) treść”.
    nadawca_wl: bool,
    grupowy: u32,
    grupowy_wl: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DoMnie {
    Wlasny,
    Grupa,
}

impl DoMnie {
    fn tlo(self) -> egui::Color32 {
        match self {
            DoMnie::Wlasny => TAB_DO_MNIE,
            DoMnie::Grupa => TAB_DO_GRUPY,
        }
    }

    fn opis(self) -> &'static str {
        match self {
            DoMnie::Wlasny => "do mnie",
            DoMnie::Grupa => "do mojej grupy",
        }
    }
}

/// Odebrana wiadomość na RIC programu. Jak w pagerze liczy się tylko RIC: funkcja wybiera
/// rodzaj sygnału, a nie odbiorcę.
fn do_mnie(w: &Wpis, m: &MojeRic) -> Option<DoMnie> {
    if w.nadane {
        None
    } else if m.wlasny_wl && w.ric == m.wlasny {
        Some(DoMnie::Wlasny)
    } else if m.grupowy_wl && w.ric == m.grupowy {
        Some(DoMnie::Grupa)
    } else {
        None
    }
}

/// Kolumny tabeli historii; kolejność ustawia użytkownik, przeciągając nagłówki.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kol {
    Godzina,
    Kierunek,
    Nadawca,
    Odbiorca,
    Tresc,
    Ric,
    Funkcja,
    Predkosc,
    Stan,
}

impl Kol {
    const DOMYSLNE: [Kol; 9] = [
        Kol::Godzina,
        Kol::Kierunek,
        Kol::Nadawca,
        Kol::Odbiorca,
        Kol::Tresc,
        Kol::Ric,
        Kol::Funkcja,
        Kol::Predkosc,
        Kol::Stan,
    ];

    fn naglowek(self) -> &'static str {
        match self {
            Kol::Godzina => "Godzina",
            Kol::Kierunek => "Kierunek",
            Kol::Nadawca => "Nadawca",
            Kol::Odbiorca => "Odbiorca",
            Kol::Tresc => "Treść",
            Kol::Ric => "RIC",
            Kol::Funkcja => "Funkcja",
            Kol::Predkosc => "bit/s",
            Kol::Stan => "Stan",
        }
    }

    /// Nazwa w pliku ustawień.
    fn klucz(self) -> &'static str {
        match self {
            Kol::Godzina => "godzina",
            Kol::Kierunek => "kierunek",
            Kol::Nadawca => "nadawca",
            Kol::Odbiorca => "odbiorca",
            Kol::Tresc => "tresc",
            Kol::Ric => "ric",
            Kol::Funkcja => "funkcja",
            Kol::Predkosc => "predkosc",
            Kol::Stan => "stan",
        }
    }

    /// Ostatnia kolumna bierze resztę szerokości (tak działa tabela), pozostałe mają swoją.
    fn kolumna(self, ostatnia: bool) -> Column {
        if ostatnia {
            return Column::remainder().at_least(90.0).clip(true);
        }
        match self {
            Kol::Godzina => Column::exact(68.0),
            Kol::Kierunek => Column::exact(80.0),
            Kol::Nadawca => Column::initial(110.0).at_least(60.0).clip(true),
            Kol::Odbiorca => Column::initial(130.0).at_least(60.0).clip(true),
            Kol::Tresc => Column::initial(300.0).at_least(120.0).clip(true),
            Kol::Ric => Column::exact(74.0),
            Kol::Funkcja => Column::exact(64.0),
            Kol::Predkosc => Column::exact(56.0),
            Kol::Stan => Column::initial(110.0).at_least(60.0).clip(true),
        }
    }
}

/// Kolejność z pliku ustawień: znane kolumny, każda raz. Brakujące (np. nowa kolumna po
/// aktualizacji) wchodzą przed swoją domyślną następczynią, więc zapisany układ zostaje.
fn kolumny_z_tekstu(t: &str) -> Option<Vec<Kol>> {
    let mut l: Vec<Kol> =
        t.split(',').map(|k| Kol::DOMYSLNE.into_iter().find(|c| c.klucz() == k.trim())).collect::<Option<_>>()?;
    if l.is_empty() || l.iter().enumerate().any(|(i, k)| l[..i].contains(k)) {
        return None;
    }
    for (i, k) in Kol::DOMYSLNE.iter().enumerate() {
        if !l.contains(k) {
            let poz = Kol::DOMYSLNE[i + 1..].iter().find_map(|n| l.iter().position(|x| x == n)).unwrap_or(l.len());
            l.insert(poz, *k);
        }
    }
    Some(l)
}

/// Przeciągany nagłówek: numer kolumny, z której ruszył.
struct PrzeciaganaKolumna(usize);

#[derive(Clone, PartialEq)]
struct Ustawienia {
    wejscie: String,
    wyjscie: String,
    ptt: UstawieniaPtt,
    par: Parametry,
    ric: u32,
    funkcja: u8,
    rodzaj: Rodzaj,
    kolumny: Vec<Kol>,
    moje: MojeRic,
    /// Sterowanie częstotliwością radia (osobny port CAT albo ten sam co PTT).
    radio: UstawieniaCat,
    /// Ostatnio ustawione częstotliwości (Hz), najnowsza pierwsza.
    ostatnie_czest: Vec<u64>,
}

fn plik_ustawien() -> Option<PathBuf> {
    let mut p = PathBuf::from(std::env::var_os("APPDATA")?);
    p.push("koder-dekoder-pogsag.txt");
    Some(p)
}

impl Ustawienia {
    fn domyslne() -> Self {
        let (wej, wyj) = silnik::domyslne();
        Self {
            wejscie: wej.unwrap_or_else(|| BEZ_NASLUCHU.into()),
            wyjscie: wyj.unwrap_or_default(),
            ptt: UstawieniaPtt { rodzaj: RodzajPtt::Brak, port: String::new(), predkosc: 9600, adres_civ: 0x88, odwroc: false },
            par: Parametry { predkosc: 1200, poziom: 0.5, odwroc: false, powtorzenia: 1, opoznienie_ms: 300, ogon_ms: 100, korekcja: Korekcja::Brak },
            ric: 1234567,
            funkcja: 3,
            rodzaj: Rodzaj::Tekst,
            kolumny: Kol::DOMYSLNE.to_vec(),
            moje: MojeRic { wlasny: 0, wlasny_wl: false, nadawca_wl: true, grupowy: 0, grupowy_wl: false },
            radio: UstawieniaCat { rodzaj: RodzajCat::Brak, port: String::new(), predkosc: 9600, adres_civ: 0x88 },
            ostatnie_czest: Vec::new(),
        }
    }

    /// Plik `klucz=wartość`; brakujące albo złe wartości zostają domyślne.
    fn wczytaj() -> Self {
        let mut u = Self::domyslne();
        let tekst = plik_ustawien().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        for l in tekst.lines() {
            let Some((k, v)) = l.split_once('=') else { continue };
            let liczba = || v.trim().parse::<f64>().ok();
            match k.trim() {
                "wejscie" => u.wejscie = v.to_string(),
                "wyjscie" => u.wyjscie = v.to_string(),
                "ptt" => u.ptt.rodzaj = RodzajPtt::z_klucza(v.trim()).unwrap_or(u.ptt.rodzaj),
                "port" => u.ptt.port = v.trim().to_string(),
                "cat_predkosc" => u.ptt.predkosc = liczba().map_or(u.ptt.predkosc, |x| x as u32),
                "civ" => u.ptt.adres_civ = u8::from_str_radix(v.trim(), 16).unwrap_or(u.ptt.adres_civ),
                "ptt_odwroc" => u.ptt.odwroc = v.trim() == "1",
                "predkosc" => {
                    u.par.predkosc = liczba().map(|x| x as u32).filter(|p| pocsag::PREDKOSCI.contains(p)).unwrap_or(u.par.predkosc)
                }
                "poziom" => u.par.poziom = liczba().map_or(u.par.poziom, |x| (x as f32).clamp(0.01, 1.0)),
                "polaryzacja_odwr" => u.par.odwroc = v.trim() == "1",
                "korekta_7100" => u.par.korekcja = if v.trim() == "1" { Korekcja::Ic7100Usb } else { Korekcja::Brak },
                "powtorzenia" => u.par.powtorzenia = liczba().map_or(u.par.powtorzenia, |x| (x as u32).clamp(1, 5)),
                "opoznienie_ms" => u.par.opoznienie_ms = liczba().map_or(u.par.opoznienie_ms, |x| (x as u32).min(3000)),
                "ogon_ms" => u.par.ogon_ms = liczba().map_or(u.par.ogon_ms, |x| (x as u32).min(2000)),
                "ric" => u.ric = liczba().map_or(u.ric, |x| (x as u32).min(MAX_RIC)),
                "funkcja" => u.funkcja = liczba().map_or(u.funkcja, |x| (x as u8).min(3)),
                "typ" => {
                    u.rodzaj = Rodzaj::WSZYSTKIE.into_iter().find(|r| r.nazwa() == v.trim()).unwrap_or(u.rodzaj);
                }
                "kolumny" => u.kolumny = kolumny_z_tekstu(v).unwrap_or(u.kolumny),
                "moj_ric" => u.moje.wlasny = liczba().map_or(u.moje.wlasny, |x| (x as u32).min(MAX_RIC)),
                "moj_ric_wl" => u.moje.wlasny_wl = v.trim() == "1",
                "ric_grupowy" => u.moje.grupowy = liczba().map_or(u.moje.grupowy, |x| (x as u32).min(MAX_RIC)),
                "ric_grupowy_wl" => u.moje.grupowy_wl = v.trim() == "1",
                "moj_ric_nadawca" => u.moje.nadawca_wl = v.trim() == "1",
                "radio_cat" => u.radio.rodzaj = RodzajCat::z_klucza(v.trim()).unwrap_or(u.radio.rodzaj),
                "radio_port" => u.radio.port = v.trim().to_string(),
                "radio_bd" => u.radio.predkosc = liczba().map_or(u.radio.predkosc, |x| x as u32),
                "radio_civ" => u.radio.adres_civ = u8::from_str_radix(v.trim(), 16).unwrap_or(u.radio.adres_civ),
                "ostatnie_czest" => {
                    u.ostatnie_czest =
                        v.split(',').filter_map(|x| x.trim().parse::<u64>().ok()).take(czestotliwosci::OSTATNICH).collect()
                }
                _ => {}
            }
        }
        u
    }

    fn zapisz(&self) {
        let b = |x: bool| if x { "1" } else { "0" };
        let t = format!(
            "wejscie={}\nwyjscie={}\nptt={}\nport={}\ncat_predkosc={}\nciv={:02X}\nptt_odwroc={}\npredkosc={}\npoziom={}\n\
             polaryzacja_odwr={}\npowtorzenia={}\nopoznienie_ms={}\nogon_ms={}\nric={}\nfunkcja={}\ntyp={}\nkolumny={}\n\
             moj_ric={}\nmoj_ric_wl={}\nric_grupowy={}\nric_grupowy_wl={}\nmoj_ric_nadawca={}\nkorekta_7100={}\n\
             radio_cat={}\nradio_port={}\nradio_bd={}\nradio_civ={:02X}\nostatnie_czest={}\n",
            self.wejscie,
            self.wyjscie,
            self.ptt.rodzaj.klucz(),
            self.ptt.port,
            self.ptt.predkosc,
            self.ptt.adres_civ,
            b(self.ptt.odwroc),
            self.par.predkosc,
            self.par.poziom,
            b(self.par.odwroc),
            self.par.powtorzenia,
            self.par.opoznienie_ms,
            self.par.ogon_ms,
            self.ric,
            self.funkcja,
            self.rodzaj.nazwa(),
            self.kolumny.iter().map(|k| k.klucz()).collect::<Vec<_>>().join(","),
            self.moje.wlasny,
            b(self.moje.wlasny_wl),
            self.moje.grupowy,
            b(self.moje.grupowy_wl),
            b(self.moje.nadawca_wl),
            b(self.par.korekcja == Korekcja::Ic7100Usb),
            self.radio.rodzaj.klucz(),
            self.radio.port,
            self.radio.predkosc,
            self.radio.adres_civ,
            self.ostatnie_czest.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
        );
        if let Some(p) = plik_ustawien() {
            let _ = std::fs::write(p, t);
        }
    }
}

struct Aplikacja {
    u: Ustawienia,
    zapisane: Ustawienia,
    wejscia: Vec<String>,
    wyjscia: Vec<String>,
    porty: Vec<(String, String)>,
    wspolne: Arc<Wspolne>,
    silnik: Option<Silnik>,
    blad_startu: Option<String>,
    /// Linia konsoli.
    tekst: String,
    /// Wysłane wcześniej teksty, do przywołania strzałkami.
    poprzednie: Vec<String>,
    indeks: Option<usize>,
    /// Komunikat konsoli: tekst i czy to błąd.
    komunikat: Option<(String, bool)>,
    komunikat_pliku: Arc<Mutex<String>>,
    fokus: bool,
    /// Ile wpisów historii już sprawdzono pod kątem wiadomości do mnie (mruganie paska zadań).
    sprawdzone: usize,
    odbiorcy: Vec<Odbiorca>,
    odbiorcy_zapisani: Vec<Odbiorca>,
    okno_odbiorcow: bool,
    blad_odbiorcow: Option<String>,
    /// Pole MHz w wierszu „Radio”.
    czest_tekst: String,
    czestotliwosci: Vec<Czest>,
    czestotliwosci_zapisane: Vec<Czest>,
    okno_czest: bool,
    blad_czest: Option<String>,
}

impl Aplikacja {
    fn new() -> Self {
        let u = Ustawienia::wczytaj();
        // W polu MHz ostatnio ustawiona; samo uruchomienie programu radia nie przestraja.
        let u_czest_tekst = u.ostatnie_czest.first().map(|&h| ptt::tekst_czest(h)).unwrap_or_default();
        let mut a = Self {
            zapisane: u.clone(),
            u,
            wejscia: Vec::new(),
            wyjscia: Vec::new(),
            porty: Vec::new(),
            wspolne: Wspolne::new(),
            silnik: None,
            blad_startu: None,
            tekst: String::new(),
            poprzednie: Vec::new(),
            indeks: None,
            komunikat: None,
            komunikat_pliku: Arc::default(),
            fokus: true,
            odbiorcy: odbiorcy::wczytaj(),
            odbiorcy_zapisani: Vec::new(),
            okno_odbiorcow: false,
            blad_odbiorcow: None,
            czest_tekst: u_czest_tekst,
            czestotliwosci: czestotliwosci::wczytaj(),
            czestotliwosci_zapisane: Vec::new(),
            okno_czest: false,
            blad_czest: None,
            sprawdzone: 0,
        };
        a.czestotliwosci_zapisane = a.czestotliwosci.clone();
        // Historia wczytana z pliku dnia jest już stara: nie budzi paska zadań.
        a.sprawdzone = a.wspolne.historia.lock().unwrap().len();
        a.odbiorcy_zapisani = a.odbiorcy.clone();
        a.odswiez();
        a
    }

    /// Listy urządzeń i portów; zapamiętane, a odłączone urządzenie zastępuje domyślne systemowe.
    fn odswiez(&mut self) {
        let (dom_wej, dom_wyj) = silnik::domyslne();
        self.wejscia = std::iter::once(BEZ_NASLUCHU.to_string()).chain(silnik::urzadzenia_wejsciowe()).collect();
        self.wyjscia = silnik::urzadzenia_wyjsciowe();
        self.porty = ptt::porty();
        if !self.wejscia.contains(&self.u.wejscie) {
            self.u.wejscie = dom_wej.unwrap_or_else(|| BEZ_NASLUCHU.into());
        }
        if !self.wyjscia.contains(&self.u.wyjscie) {
            self.u.wyjscie = dom_wyj.unwrap_or_default();
        }
    }

    fn przelacz(&mut self) {
        if self.silnik.take().is_some() {
            return;
        }
        match Silnik::start(&self.u.wejscie, &self.u.wyjscie, &self.u.ptt, &self.u.radio, self.wspolne.clone()) {
            Ok(s) => {
                self.silnik = Some(s);
                self.blad_startu = None;
                self.fokus = true;
            }
            Err(e) => self.blad_startu = Some(e),
        }
    }

    fn nadaje(&self) -> bool {
        self.wspolne.tx.lock().unwrap().nadaje
    }

    /// Linia konsoli z numerem nadawcy na początku, gdy jest włączony (sam sygnał nie ma treści).
    fn tekst_do_nadania(&self) -> String {
        let m = &self.u.moje;
        if m.wlasny_wl && m.nadawca_wl && self.u.rodzaj != Rodzaj::Sygnal && !self.tekst.trim().is_empty() {
            koder::z_nadawca(m.wlasny, &self.tekst)
        } else {
            self.tekst.clone()
        }
    }

    fn wyslij(&mut self) {
        if self.silnik.is_none() {
            self.komunikat = Some(("Najpierw Start: otwiera kartę dźwiękową i port PTT".into(), true));
            return;
        }
        let pelny = self.tekst_do_nadania();
        match koder::przygotuj(self.u.rodzaj, &pelny) {
            Err(e) => self.komunikat = Some((e, true)),
            Ok(tresc) => {
                let w = DoNadania { ric: self.u.ric, funkcja: self.u.funkcja, rodzaj: self.u.rodzaj, tresc };
                self.wspolne.zlec(Zlecenie::Wiadomosc { w, wpisane: pelny.trim().to_string(), par: self.u.par });
                let wpisane = self.tekst.trim().to_string();
                if !wpisane.is_empty() && self.poprzednie.last() != Some(&wpisane) {
                    self.poprzednie.push(wpisane);
                }
                self.tekst.clear();
                self.indeks = None;
                self.komunikat = None;
            }
        }
    }

    fn ustawienia(&mut self, ui: &mut egui::Ui) {
        let dziala = self.silnik.is_some();
        egui::Grid::new("ustawienia").num_columns(2).spacing([10.0, 5.0]).min_col_width(120.0).show(ui, |ui| {
            wybor(ui, "Wejście (odbiór)", &self.wejscia, &mut self.u.wejscie, !dziala);
            wybor(ui, "Wyjście (nadawanie)", &self.wyjscia, &mut self.u.wyjscie, !dziala);

            ui.label("PTT");
            ui.add_enabled_ui(!dziala, |ui| {
                egui::ComboBox::from_id_salt("ptt")
                    .selected_text(self.u.ptt.rodzaj.nazwa())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for r in RodzajPtt::WSZYSTKIE {
                            ui.selectable_value(&mut self.u.ptt.rodzaj, r, r.nazwa());
                        }
                    });
            });
            ui.end_row();

            let rodzaj = self.u.ptt.rodzaj;
            if rodzaj.uzywa_portu() {
                ui.label("Port COM");
                ui.add_enabled_ui(!dziala, |ui| {
                    wybor_portu(ui, "port", &self.porty, &mut self.u.ptt.port);
                });
                ui.end_row();
            }
            if rodzaj.cat() {
                ui.label("Prędkość CAT");
                ui.add_enabled_ui(!dziala, |ui| {
                    egui::ComboBox::from_id_salt("cat")
                        .selected_text(format!("{} bit/s", self.u.ptt.predkosc))
                        .show_ui(ui, |ui| {
                            for p in PREDKOSCI_CAT {
                                ui.selectable_value(&mut self.u.ptt.predkosc, p, format!("{p} bit/s"));
                            }
                        });
                });
                ui.end_row();
            }
            if rodzaj == RodzajPtt::CatIcom {
                ui.label("Adres CI-V");
                ui.add_enabled_ui(!dziala, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.u.ptt.adres_civ).hexadecimal(2, false, true).suffix("h"));
                        ui.label(egui::RichText::new("IC-7100: 88h, IC-705: A4h, IC-9700: A2h").weak().small());
                    });
                });
                ui.end_row();
            }
            if rodzaj.linia() {
                ui.label("Linia PTT");
                ui.add_enabled(!dziala, egui::Checkbox::new(&mut self.u.ptt.odwroc, "odwrócona (aktywna stanem niskim)"));
                ui.end_row();
            }

            ui.label("Sterowanie radiem").on_hover_text(
                "Częstotliwość ustawiana z programu (wiersz „Radio” nad konsolą).\nTryb, menu i reszta radia zostają ręczne.",
            );
            ui.add_enabled_ui(!dziala, |ui| {
                egui::ComboBox::from_id_salt("radio_cat")
                    .selected_text(self.u.radio.rodzaj.nazwa())
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for r in RodzajCat::WSZYSTKIE {
                            ui.selectable_value(&mut self.u.radio.rodzaj, r, r.nazwa());
                        }
                    });
            });
            ui.end_row();
            if self.u.radio.rodzaj != RodzajCat::Brak {
                ui.label("Port CAT radia");
                ui.add_enabled_ui(!dziala, |ui| {
                    wybor_portu(ui, "radio_port", &self.porty, &mut self.u.radio.port);
                });
                ui.end_row();
                let etykieta = self.porty.iter().find(|(n, _)| *n == self.u.radio.port).map_or("", |(_, e)| e.as_str());
                if self.u.radio.rodzaj == RodzajCat::Yaesu && etykieta.contains("Standard COM Port") {
                    ui.label("");
                    ui.colored_label(CZERWONY, "To port Standard (PTT). CAT Yaesu idzie przez port Enhanced (FT-991A: COM4)");
                    ui.end_row();
                }
                ui.label("Prędkość CAT radia");
                ui.add_enabled_ui(!dziala, |ui| {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("radio_bd").selected_text(format!("{} bit/s", self.u.radio.predkosc)).show_ui(
                            ui,
                            |ui| {
                                for p in PREDKOSCI_CAT {
                                    ui.selectable_value(&mut self.u.radio.predkosc, p, format!("{p} bit/s"));
                                }
                            },
                        );
                        if self.u.ptt.rodzaj.uzywa_portu() && self.u.ptt.port == self.u.radio.port {
                            ui.label(egui::RichText::new("ten sam port co PTT").weak().small());
                        }
                    });
                });
                ui.end_row();
                if self.u.radio.rodzaj == RodzajCat::Icom {
                    ui.label("Adres CI-V radia");
                    ui.add_enabled_ui(!dziala, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut self.u.radio.adres_civ).hexadecimal(2, false, true).suffix("h"));
                            ui.label(egui::RichText::new("IC-7100: 88h, IC-705: A4h, IC-9700: A2h").weak().small());
                        });
                    });
                    ui.end_row();
                }
            }

            ui.label("Opóźnienie PTT");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.u.par.opoznienie_ms).range(0..=3000).speed(5.0).suffix(" ms"));
                ui.label(egui::RichText::new("od PTT do danych").weak().small());
            });
            ui.end_row();
            ui.label("Ogon");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.u.par.ogon_ms).range(0..=2000).speed(5.0).suffix(" ms"));
                ui.label(egui::RichText::new("cisza przed puszczeniem PTT").weak().small());
            });
            ui.end_row();

            ui.label("Poziom wyjścia");
            let mut proc = (self.u.par.poziom * 100.0).round();
            if ui.add(egui::Slider::new(&mut proc, 1.0..=100.0).integer().suffix(" %")).changed() {
                self.u.par.poziom = proc / 100.0;
            }
            ui.end_row();

            ui.label("Prędkość POCSAG");
            ui.horizontal(|ui| {
                for p in pocsag::PREDKOSCI {
                    ui.selectable_value(&mut self.u.par.predkosc, p, format!("{p} bit/s"));
                }
            });
            ui.end_row();
            ui.label("Polaryzacja");
            ui.checkbox(&mut self.u.par.odwroc, "odwrócona (gdy pager nie reaguje)");
            ui.end_row();
            ui.label("Korekta 7100");
            let mut k7100 = self.u.par.korekcja == Korekcja::Ic7100Usb;
            if ui
                .checkbox(&mut k7100, "korekta 7100 (nadawanie przez USB IC-7100)")
                .on_hover_text(
                    "IC-7100 przez USB (też w FM-D) podbija wysokie tony i obcina dół: bity robią się szpilkami,\n\
                     środek bitu ma tylko ~0,8 kHz dewiacji. Korekta odwraca ten tor (pomiar SDR 06.11:\n\
                     środek bitu ~3,5 kHz). Dla innego radia (np. FT-991A) wyłącz.",
                )
                .changed()
            {
                self.u.par.korekcja = if k7100 { Korekcja::Ic7100Usb } else { Korekcja::Brak };
            }
            ui.end_row();
            ui.label("Powtórzenia");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.u.par.powtorzenia).range(1..=5).suffix("×"));
                ui.label(egui::RichText::new("całe nadanie pod jednym PTT").weak().small());
            });
            ui.end_row();
            ui.label("Mój pager")
                .on_hover_text("Program jak pager: odebrane wiadomości na te RIC są podświetlone w historii");
            ui.horizontal(|ui| {
                let m = &mut self.u.moje;
                ui.checkbox(&mut m.wlasny_wl, "własny RIC");
                ui.add_enabled(m.wlasny_wl, egui::DragValue::new(&mut m.wlasny).range(0..=MAX_RIC).speed(1.0));
                ui.add_enabled(m.wlasny_wl, egui::Checkbox::new(&mut m.nadawca_wl, "dopisuj jako nadawcę"))
                    .on_hover_text("Na początku każdej wysłanej wiadomości: (własny RIC) treść");
                ui.add_space(16.0);
                ui.checkbox(&mut m.grupowy_wl, "RIC grupowy");
                ui.add_enabled(m.grupowy_wl, egui::DragValue::new(&mut m.grupowy).range(0..=MAX_RIC).speed(1.0));
            });
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(!dziala, egui::Button::new("Odśwież listy")).on_hover_text("Karty dźwiękowe i porty COM").clicked() {
                self.odswiez();
            }
            let probny = ui
                .add_enabled(dziala && !self.nadaje(), egui::Button::new(format!("Ton próbny {TON_PROBNY_S:.0} s")))
                .on_hover_text("Sama preambuła 1010... z PTT: do ustawienia poziomu i dewiacji");
            if probny.clicked() {
                self.wspolne.zlec(Zlecenie::Probny { sekundy: TON_PROBNY_S, par: self.u.par });
            }
        });
    }

    fn status(&self, ui: &mut egui::Ui) {
        let tx = self.wspolne.tx.lock().unwrap().clone();
        let nasluch = self.silnik.as_ref().is_some_and(|s| s.nasluch);
        let (tekst, kolor) = match &self.silnik {
            None => ("Zatrzymany", ui.visuals().weak_text_color()),
            Some(_) if tx.nadaje => ("NADAWANIE", CZERWONY),
            Some(_) if nasluch => ("Nasłuch POCSAG", ZIELONY),
            Some(_) => ("Gotowy do nadawania (bez nasłuchu)", ZOLTY),
        };
        ui.label(egui::RichText::new(tekst).size(22.0).color(kolor).strong());
        let w_kolejce = self.wspolne.w_kolejce();
        if tx.nadaje || w_kolejce > 0 {
            ui.horizontal(|ui| {
                let szer = ui.available_width() - 90.0;
                let opis = if w_kolejce > 0 { format!("{}  (w kolejce: {w_kolejce})", tx.opis) } else { tx.opis.clone() };
                ui.add(egui::ProgressBar::new(self.wspolne.postep()).desired_width(szer).text(opis));
                if ui.button("Przerwij").on_hover_text("Puszcza PTT i czyści kolejkę").clicked() {
                    self.wspolne.przerwij();
                }
            });
        } else if !tx.opis.is_empty() {
            ui.label(egui::RichText::new(&tx.opis).weak());
        }
        if nasluch {
            let p = self.wspolne.poziom();
            ui.add(egui::ProgressBar::new((p * 1.5).min(1.0)).text("Poziom wejścia"));
            if p >= 0.99 {
                ui.colored_label(CZERWONY, "Wejście przesterowane: przycisz odbiornik");
            }
            let (luk, ms) = self.wspolne.luki();
            if luk > 0 {
                ui.colored_label(
                    ZOLTY,
                    egui::RichText::new(format!(
                        "Karta zgubiła próbki wejścia: {luk}× (razem {ms} ms). Komputer obciążony; wiadomość nadana w tej chwili mogła zginąć."
                    ))
                    .small(),
                );
            }
        }
        if let Some(b) = self.wspolne.blad.lock().unwrap().as_ref() {
            ui.colored_label(CZERWONY, b);
        }
    }

    fn konsola(&mut self, ui: &mut egui::Ui) {
        let mut wybrany: Option<Odbiorca> = None;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Konsola").strong());
            ui.add_space(8.0);
            ui.label("Odbiorca");
            let nazwa = odbiorcy::nazwa_dokladna(&self.odbiorcy, self.u.ric, self.u.funkcja).unwrap_or("— wpisz RIC —").to_string();
            egui::ComboBox::from_id_salt("odbiorca").selected_text(nazwa).width(170.0).show_ui(ui, |ui| {
                if self.odbiorcy.is_empty() {
                    ui.label("Lista pusta: dodaj w „Odbiorcy...”");
                }
                for o in &self.odbiorcy {
                    let ten = o.ric == self.u.ric && o.funkcja == self.u.funkcja;
                    if ui.selectable_label(ten, format!("{}  (RIC {})", o.nazwa, o.ric)).clicked() {
                        wybrany = Some(o.clone());
                    }
                }
            });
            if ui.button("Odbiorcy...").on_hover_text("Lista odbiorców: nazwa, RIC, funkcja, typ, prędkość").clicked() {
                self.okno_odbiorcow = !self.okno_odbiorcow;
            }
            ui.add_space(8.0);
            ui.label("RIC");
            ui.add(egui::DragValue::new(&mut self.u.ric).range(0..=MAX_RIC).speed(1.0));
            ui.label("Funkcja");
            egui::ComboBox::from_id_salt("funkcja")
                .selected_text(funkcja_opis(self.u.funkcja))
                .width(64.0)
                .show_ui(ui, |ui| {
                    for f in 0..4u8 {
                        ui.selectable_value(&mut self.u.funkcja, f, funkcja_opis(f));
                    }
                });
            ui.label("Typ");
            egui::ComboBox::from_id_salt("typ").selected_text(self.u.rodzaj.nazwa()).width(96.0).show_ui(ui, |ui| {
                for r in Rodzaj::WSZYSTKIE {
                    ui.selectable_value(&mut self.u.rodzaj, r, r.nazwa());
                }
            });
        });
        if let Some(o) = wybrany {
            self.uzyj_odbiorcy(&o);
        }
        let podpowiedz = match self.u.rodzaj {
            Rodzaj::Tekst => "Wpisz wiadomość, Enter wysyła",
            Rodzaj::Cyfry => "Cyfry 0-9, spacja, - U * ( ), Enter wysyła",
            Rodzaj::Sygnal => "Sam sygnał: treść pomijana, Enter wysyła",
        };
        let mut wyslij = false;
        let pole = ui
            .horizontal(|ui| {
                let szer = (ui.available_width() - 88.0).max(100.0);
                let r = ui.add_sized(
                    [szer, 28.0],
                    egui::TextEdit::singleline(&mut self.tekst)
                        .font(egui::TextStyle::Monospace)
                        .hint_text(podpowiedz)
                        .char_limit(MAX_ZNAKOW),
                );
                if ui.add_sized([80.0, 28.0], egui::Button::new("Wyślij")).clicked() {
                    wyslij = true;
                }
                r
            })
            .inner;
        if pole.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            wyslij = true;
        }
        if pole.has_focus() && !self.poprzednie.is_empty() {
            let (gora, dol) = ui.input(|i| (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown)));
            if gora || dol {
                let n = self.poprzednie.len();
                self.indeks = match (self.indeks, gora) {
                    (None, true) => Some(n - 1),
                    (Some(i), true) => Some(i.saturating_sub(1)),
                    (Some(i), false) if i + 1 < n => Some(i + 1),
                    _ => None,
                };
                self.tekst = self.indeks.map_or_else(String::new, |i| self.poprzednie[i].clone());
                kursor_na_koniec(ui.ctx(), pole.id, &self.tekst);
            }
        }
        if pole.changed() {
            self.komunikat = None;
        }
        if wyslij {
            self.wyslij();
            pole.request_focus();
        } else if self.fokus {
            pole.request_focus();
            self.fokus = false;
        }
        self.info_konsoli(ui);
    }

    /// Pod linią: licznik znaków, czas nadania, ostrzeżenia o znakach.
    fn info_konsoli(&self, ui: &mut egui::Ui) {
        if let Some((k, blad)) = &self.komunikat {
            ui.colored_label(if *blad { CZERWONY } else { ZIELONY }, k);
            return;
        }
        let pusty = self.tekst.trim().is_empty();
        match koder::przygotuj(self.u.rodzaj, &self.tekst_do_nadania()) {
            Err(e) if !pusty => {
                ui.colored_label(CZERWONY, e);
            }
            Err(_) => {
                let t = if self.poprzednie.is_empty() { "Enter wysyła" } else { "Enter wysyła, strzałki góra/dół: poprzednie wiadomości" };
                ui.label(egui::RichText::new(t).weak().small());
            }
            Ok(tresc) => {
                let w = DoNadania { ric: self.u.ric, funkcja: self.u.funkcja, rodzaj: self.u.rodzaj, tresc: tresc.clone() };
                let par = &self.u.par;
                let bitow = koder::bity(&[w]).len() * par.powtorzenia as usize;
                let czas = koder::czas_s(bitow, par.predkosc) + par.opoznienie_ms as f32 / 1000.0;
                let mut t = format!(
                    "{}/{MAX_ZNAKOW} znaków, nadanie ok. {} s",
                    tresc.chars().count(),
                    format!("{czas:.1}").replace('.', ",")
                );
                if self.u.rodzaj != Rodzaj::Sygnal && tresc != self.tekst.trim() {
                    t += &format!(", pójdzie jako: {tresc}");
                }
                ui.label(egui::RichText::new(t).weak().small());
            }
        }
    }

    fn historia(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Historia").strong());
            ui.label(egui::RichText::new("nowe na górze, prawy przycisk na wierszu: kopiuj").weak().small());
            let (wl, gr) = {
                let h = self.wspolne.historia.lock().unwrap();
                let ile = |d| h.iter().filter(|w| do_mnie(w, &self.u.moje) == Some(d)).count();
                (ile(DoMnie::Wlasny), ile(DoMnie::Grupa))
            };
            if self.u.moje.wlasny_wl {
                ui.label(egui::RichText::new(format!("  do mnie: {wl}")).background_color(TAB_DO_MNIE).color(TAB_TEKST));
            }
            if self.u.moje.grupowy_wl {
                ui.label(egui::RichText::new(format!("  do grupy: {gr}")).background_color(TAB_DO_GRUPY).color(TAB_TEKST));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Wyczyść okno").on_hover_text("Plik dnia zostaje bez zmian").clicked() {
                    self.wspolne.historia.lock().unwrap().clear();
                }
                if ui.button("Plik dnia").on_hover_text("Otwiera dzisiejszy plik historii w Notatniku").clicked() {
                    self.otworz_plik_dnia();
                }
                if ui.button("Dekoduj plik...").on_hover_text("POCSAG z nagrania WAV/MP3/FLAC/OGG").clicked() {
                    self.dekoduj_plik();
                }
                if ui
                    .button("Kopiuj tabelę")
                    .on_hover_text("Nagłówek i wszystkie wiersze, rozdzielone tabulatorami — do wklejenia w arkuszu")
                    .clicked()
                {
                    let h = self.wspolne.historia.lock().unwrap();
                    ui.ctx().copy_text(tekst_tabeli(&self.u.kolumny, &h, &self.odbiorcy, &self.u.moje));
                }
            });
        });
        let kom = self.komunikat_pliku.lock().unwrap().clone();
        if !kom.is_empty() {
            ui.label(egui::RichText::new(kom).weak().small());
        }
        let wspolne = self.wspolne.clone();
        let h = wspolne.historia.lock().unwrap();
        let mut klik = None;
        egui::Frame::new().fill(egui::Color32::WHITE).stroke(egui::Stroke::new(1.0, TAB_SIATKA)).show(ui, |ui| {
            tabela_historii(ui, &h, &self.odbiorcy, &self.u.moje, &mut self.u.kolumny, &mut klik);
        });
        drop(h);
        if let Some((ric, funkcja)) = klik {
            self.u.ric = ric;
            self.u.funkcja = funkcja;
            self.fokus = true;
        }
    }

    /// Nowa wiadomość na RIC programu: pasek zadań mruga, jak pager, który piszczy.
    fn nowe_do_mnie(&mut self, ctx: &egui::Context) {
        let h = self.wspolne.historia.lock().unwrap();
        if h.len() < self.sprawdzone {
            // „Wyczyść okno” albo przycięcie listy.
            self.sprawdzone = h.len();
            return;
        }
        let nowa = h[self.sprawdzone..].iter().any(|w| do_mnie(w, &self.u.moje).is_some());
        self.sprawdzone = h.len();
        drop(h);
        if nowa {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
        }
    }

    /// Wiersz „Radio”: lista częstotliwości, ostatnio użyte i ręczne MHz.
    fn radio(&mut self, ui: &mut egui::Ui) {
        let wlaczone = self.u.radio.rodzaj != RodzajCat::Brak;
        let mut ustaw: Option<u64> = None;
        let mut blad: Option<String> = None;
        let opis = |c: &[Czest], hz: u64| match c.iter().find(|c| c.hz == hz) {
            Some(c) => format!("{} MHz   {}", ptt::tekst_czest(hz), c.nazwa),
            None => format!("{} MHz", ptt::tekst_czest(hz)),
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Radio").strong());
            ui.add_space(8.0);
            ui.add_enabled_ui(wlaczone, |ui| {
                egui::ComboBox::from_id_salt("lista_czest").selected_text("Lista").width(80.0).show_ui(ui, |ui| {
                    if self.czestotliwosci.is_empty() {
                        ui.label("Lista pusta: dodaj w „Częstotliwości...”");
                    }
                    for c in &self.czestotliwosci {
                        if ui.selectable_label(false, opis(&self.czestotliwosci, c.hz)).clicked() {
                            ustaw = Some(c.hz);
                        }
                    }
                });
                egui::ComboBox::from_id_salt("ostatnie_czest").selected_text("Ostatnie").width(80.0).show_ui(ui, |ui| {
                    if self.u.ostatnie_czest.is_empty() {
                        ui.label("Jeszcze nic nie ustawiano");
                    }
                    for &hz in &self.u.ostatnie_czest {
                        if ui.selectable_label(false, opis(&self.czestotliwosci, hz)).clicked() {
                            ustaw = Some(hz);
                        }
                    }
                });
                let pole = ui.add(egui::TextEdit::singleline(&mut self.czest_tekst).desired_width(90.0).hint_text("np. 435,000"));
                ui.label("MHz");
                let enter = pole.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Ustaw").on_hover_text("Przestraja radio na częstotliwość z pola (Enter też)").clicked() || enter {
                    match ptt::czest_z_tekstu(&self.czest_tekst) {
                        Ok(hz) => ustaw = Some(hz),
                        Err(e) => blad = Some(e),
                    }
                }
            });
            if ui.button("Częstotliwości...").on_hover_text("Lista: nazwy, dodawanie, kolejność, usuwanie").clicked() {
                self.okno_czest = true;
            }
            if !wlaczone {
                ui.label(egui::RichText::new("włącz „Sterowanie radiem” w ustawieniach").weak().small());
            } else if let Some((t, b)) = self.wspolne.radio.lock().unwrap().clone() {
                ui.colored_label(if b { CZERWONY } else { ZIELONY }, t);
            }
        });
        if let Some(e) = blad {
            *self.wspolne.radio.lock().unwrap() = Some((e, true));
        }
        if let Some(hz) = ustaw {
            self.ustaw_czestotliwosc(hz);
        }
    }

    fn ustaw_czestotliwosc(&mut self, hz: u64) {
        self.czest_tekst = ptt::tekst_czest(hz);
        czestotliwosci::dopisz_ostatnia(&mut self.u.ostatnie_czest, hz);
        self.wspolne.ustaw_czestotliwosc(self.silnik.is_some(), &self.u.radio, hz);
    }

    fn okno_czestotliwosci(&mut self, ctx: &egui::Context) {
        let mut otwarte = self.okno_czest;
        let wlaczone = self.u.radio.rodzaj != RodzajCat::Brak;
        let (mut ustaw, mut usun, mut w_gore, mut dodaj) = (None, None, None, false);
        egui::Window::new("Częstotliwości").open(&mut otwarte).default_width(460.0).resizable(true).show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "Zapisuje się samo do czestotliwosci.txt obok programu. MHz wpisujesz po kliknięciu w liczbę.",
                )
                .weak()
                .small(),
            );
            if let Some(b) = &self.blad_czest {
                ui.colored_label(CZERWONY, b);
            }
            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                egui::Grid::new("lista_czestotliwosci").num_columns(5).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                    for t in ["Nazwa", "MHz", "", "", ""] {
                        ui.strong(t);
                    }
                    ui.end_row();
                    for (i, c) in self.czestotliwosci.iter_mut().enumerate() {
                        ui.add_sized([180.0, 20.0], egui::TextEdit::singleline(&mut c.nazwa));
                        let mut mhz = c.hz as f64 / 1e6;
                        let pole = egui::DragValue::new(&mut mhz)
                            .range(0.03..=1300.0)
                            .speed(0.0)
                            .custom_formatter(|v, _| ptt::tekst_czest((v * 1e6).round() as u64))
                            .custom_parser(|s| ptt::czest_z_tekstu(s).ok().map(|hz| hz as f64 / 1e6));
                        if ui.add_sized([90.0, 20.0], pole).changed() {
                            c.hz = (mhz * 1e6).round() as u64;
                        }
                        if ui.add_enabled(wlaczone, egui::Button::new("Ustaw")).clicked() {
                            ustaw = Some(c.hz);
                        }
                        if ui.add_enabled(i > 0, egui::Button::new("▲")).on_hover_text("Wyżej na liście").clicked() {
                            w_gore = Some(i);
                        }
                        if ui.button("Usuń").clicked() {
                            usun = Some(i);
                        }
                        ui.end_row();
                    }
                });
            });
            ui.add_space(4.0);
            if ui.button("Dodaj (MHz z pola w wierszu „Radio”)").clicked() {
                dodaj = true;
            }
        });
        self.okno_czest = otwarte;
        if dodaj {
            match ptt::czest_z_tekstu(&self.czest_tekst) {
                Ok(hz) => self
                    .czestotliwosci
                    .push(Czest { nazwa: format!("Częstotliwość {}", self.czestotliwosci.len() + 1), hz }),
                Err(e) => self.blad_czest = Some(format!("Dodaj: {e}")),
            }
        }
        if let Some(i) = usun {
            self.czestotliwosci.remove(i);
        }
        if let Some(i) = w_gore {
            self.czestotliwosci.swap(i - 1, i);
        }
        if let Some(hz) = ustaw {
            self.ustaw_czestotliwosc(hz);
        }
    }

    /// Odbiorca z listy do konsoli: RIC, funkcja, typ i prędkość jego pagera.
    fn uzyj_odbiorcy(&mut self, o: &Odbiorca) {
        self.u.ric = o.ric;
        self.u.funkcja = o.funkcja;
        self.u.rodzaj = o.rodzaj;
        self.u.par.predkosc = o.predkosc;
        self.fokus = true;
    }

    fn okno_odbiorcow(&mut self, ctx: &egui::Context) {
        let mut otwarte = self.okno_odbiorcow;
        let (mut uzyj, mut usun, mut dodaj) = (None, None, false);
        egui::Window::new("Odbiorcy").open(&mut otwarte).default_width(660.0).resizable(true).show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Zapisuje się samo do odbiorcy.txt obok programu. „Użyj” wpisuje odbiorcę do konsoli.")
                    .weak()
                    .small(),
            );
            if let Some(b) = &self.blad_odbiorcow {
                ui.colored_label(CZERWONY, b);
            }
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("lista_odbiorcow").num_columns(7).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                    for t in ["Nazwa", "RIC", "Funkcja", "Typ", "Prędkość", "", ""] {
                        ui.strong(t);
                    }
                    ui.end_row();
                    for (i, o) in self.odbiorcy.iter_mut().enumerate() {
                        ui.add_sized([180.0, 20.0], egui::TextEdit::singleline(&mut o.nazwa));
                        ui.add(egui::DragValue::new(&mut o.ric).range(0..=MAX_RIC).speed(1.0));
                        egui::ComboBox::from_id_salt(("odb_f", i)).selected_text(funkcja_opis(o.funkcja)).width(64.0).show_ui(
                            ui,
                            |ui| {
                                for f in 0..4u8 {
                                    ui.selectable_value(&mut o.funkcja, f, funkcja_opis(f));
                                }
                            },
                        );
                        egui::ComboBox::from_id_salt(("odb_t", i)).selected_text(o.rodzaj.nazwa()).width(96.0).show_ui(ui, |ui| {
                            for r in Rodzaj::WSZYSTKIE {
                                ui.selectable_value(&mut o.rodzaj, r, r.nazwa());
                            }
                        });
                        egui::ComboBox::from_id_salt(("odb_p", i)).selected_text(format!("{} bit/s", o.predkosc)).width(90.0).show_ui(
                            ui,
                            |ui| {
                                for p in pocsag::PREDKOSCI {
                                    ui.selectable_value(&mut o.predkosc, p, format!("{p} bit/s"));
                                }
                            },
                        );
                        if ui.button("Użyj").clicked() {
                            uzyj = Some(i);
                        }
                        if ui.button("Usuń").clicked() {
                            usun = Some(i);
                        }
                        ui.end_row();
                    }
                });
            });
            ui.add_space(4.0);
            if ui.button("Dodaj odbiorcę (RIC, funkcja, typ i prędkość z konsoli)").clicked() {
                dodaj = true;
            }
        });
        self.okno_odbiorcow = otwarte;
        if dodaj {
            self.odbiorcy.push(Odbiorca {
                nazwa: format!("Odbiorca {}", self.odbiorcy.len() + 1),
                ric: self.u.ric,
                funkcja: self.u.funkcja,
                rodzaj: self.u.rodzaj,
                predkosc: self.u.par.predkosc,
            });
        }
        if let Some(i) = usun {
            self.odbiorcy.remove(i);
        }
        if let Some(i) = uzyj {
            let o = self.odbiorcy[i].clone();
            self.uzyj_odbiorcy(&o);
        }
    }

    fn otworz_plik_dnia(&self) {
        let (data, _) = dziennik::data_i_godzina(0.0);
        let kom = match dziennik::sciezka_dnia(&data) {
            Ok(p) if p.exists() => match std::process::Command::new("notepad.exe").arg(&p).spawn() {
                Ok(_) => String::new(),
                Err(e) => format!("Notatnik: {e}"),
            },
            Ok(p) => format!("Dzisiaj jeszcze nic nie zapisano ({})", p.display()),
            Err(e) => e.to_string(),
        };
        *self.komunikat_pliku.lock().unwrap() = kom;
    }

    fn dekoduj_plik(&self) {
        let Some(sciezka) = rfd::FileDialog::new().add_filter("Dźwięk", &["wav", "mp3", "flac", "ogg"]).pick_file() else {
            return;
        };
        let kom = self.komunikat_pliku.clone();
        let wspolne = self.wspolne.clone();
        *kom.lock().unwrap() = "Szukam POCSAG w pliku...".to_string();
        std::thread::spawn(move || {
            *kom.lock().unwrap() = przeszukaj_plik(&sciezka, &wspolne).unwrap_or_else(|e| e);
        });
    }
}

/// Wiadomości z nagrania trafiają do okna, ale nie do pliku dnia: to stare nagrania.
fn przeszukaj_plik(sciezka: &Path, wspolne: &Wspolne) -> Result<String, String> {
    let (x, fs) = plik::wczytaj(sciezka)?;
    let nazwa = sciezka.file_name().unwrap_or_default().to_string_lossy().to_string();
    let lista = pocsag::dekoduj_plik(&x, fs);
    let n = lista.len();
    for w in lista {
        let (glowna, druga) = w.odczyty();
        wspolne.dodaj_wpis(Wpis {
            godzina: format!("{:.1} s", w.probka as f32 / fs).replace('.', ","),
            nadane: false,
            ric: w.ric,
            funkcja: w.funkcja,
            predkosc: w.predkosc,
            tresc: glowna,
            druga: druga.unwrap_or_default(),
            szczegoly: w.jakosc(),
            uwaga: w.bledne_slowa > 0 || !w.zakonczona,
            zrodlo: nazwa.clone(),
            id: 0,
        });
    }
    Ok(format!("POCSAG: {n} {} w {nazwa}", if n == 1 { "wiadomość" } else { "wiadomości" }))
}

fn funkcja_opis(f: u8) -> String {
    format!("{f} ({})", (b'A' + f) as char)
}

fn kursor_na_koniec(ctx: &egui::Context, id: egui::Id, tekst: &str) {
    if let Some(mut st) = egui::TextEdit::load_state(ctx, id) {
        let c = egui::text::CCursor::new(tekst.chars().count());
        st.cursor.set_char_range(Some(egui::text::CCursorRange::one(c)));
        st.store(ctx, id);
    }
}

/// Krótki stan wiersza; pełny opis jest w dymku.
fn stan(w: &Wpis) -> (&'static str, egui::Color32) {
    let s = &w.szczegoly;
    if w.nadane {
        if s.contains("nadaję") {
            ("nadaję...", TAB_NADANE)
        } else if s.contains("BŁĄD") {
            ("BŁĄD", TAB_BLAD)
        } else if s.contains("PRZERWANE") {
            ("przerwane", TAB_UWAGA)
        } else {
            ("nadane", TAB_TEKST)
        }
    } else if w.uwaga {
        ("z błędami", TAB_UWAGA)
    } else if !w.zrodlo.is_empty() {
        ("z pliku", TAB_TEKST)
    } else if s.contains("własne nadanie") {
        ("własne (odsłuch)", TAB_TEKST)
    } else {
        ("OK", TAB_TEKST)
    }
}

/// Wartość komórki jako zwykły tekst (bez koloru), do kopiowania.
fn wartosc_komorki(k: Kol, w: &Wpis, lista: &[Odbiorca], dm: Option<DoMnie>) -> String {
    tekst_komorki(k, w, lista, dm).text().to_string()
}

/// Jeden wiersz jako tekst rozdzielony tabulatorami, w kolejności kolumn z ekranu — wklejony
/// do arkusza trafia w osobne komórki.
fn tekst_wiersza(kolumny: &[Kol], w: &Wpis, lista: &[Odbiorca], dm: Option<DoMnie>) -> String {
    kolumny.iter().map(|&k| wartosc_komorki(k, w, lista, dm)).collect::<Vec<_>>().join("\t")
}

/// Cała historia jako tekst do wklejenia w arkuszu: nagłówek i wiersze w kolejności z ekranu
/// (nowe na górze), kolumny tabulatorem, wiersze CRLF.
fn tekst_tabeli(kolumny: &[Kol], h: &[Wpis], lista: &[Odbiorca], moje: &MojeRic) -> String {
    let naglowek: String = kolumny.iter().map(|k| k.naglowek()).collect::<Vec<_>>().join("\t");
    let wiersze = h.iter().rev().map(|w| tekst_wiersza(kolumny, w, lista, do_mnie(w, moje)));
    std::iter::once(naglowek).chain(wiersze).collect::<Vec<_>>().join("\r\n")
}

fn dymek(w: &Wpis, dm: Option<DoMnie>) -> String {
    let mut t = String::new();
    if let Some(d) = dm {
        t += &format!("{}\n", d.opis().to_uppercase());
    }
    t += &format!(
        "{}  {}  RIC {}  funkcja {}  {} bit/s\n{}",
        w.godzina,
        if w.nadane { "nadane" } else { "odebrane" },
        w.ric,
        funkcja_opis(w.funkcja),
        w.predkosc,
        w.tresc
    );
    if let (Some(r), _) = koder::nadawca(&w.tresc) {
        t += &format!("\nnadawca: RIC {r}");
    }
    if !w.druga.is_empty() {
        t += &format!("\n{}", w.druga);
    }
    t += &format!("\n{}", w.szczegoly);
    if !w.zrodlo.is_empty() {
        t += &format!("\nplik: {}", w.zrodlo);
    }
    t + "\nKliknięcie: RIC i funkcja do konsoli"
}

/// Linie siatki: prawa i dolna krawędź komórki.
fn krawedzie(ui: &egui::Ui) {
    let r = ui.max_rect();
    let k = egui::Stroke::new(1.0, TAB_SIATKA);
    ui.painter().vline(r.right() - 0.5, r.y_range(), k);
    ui.painter().hline(r.x_range(), r.bottom() - 0.5, k);
}

fn komorka(row: &mut TableRow<'_, '_>, t: egui::RichText, tlo: Option<egui::Color32>) {
    row.col(|ui| {
        if let Some(c) = tlo {
            ui.painter().rect_filled(ui.max_rect(), 0.0, c);
        }
        krawedzie(ui);
        ui.add_space(5.0);
        ui.add(egui::Label::new(t).selectable(false).truncate());
    });
}

/// Treść do 3 linijek (użytkownik 07.11: „jeden wiersz mógł zwiększać się na 3 linijki”); dłuższa z „…”.
const TRESC_LINII: usize = 3;
/// Pusty margines nad i pod tekstem wiersza; wiersz jednolinijkowy ma jak dawniej 22 px.
const WIERSZ_MIN: f32 = 22.0;
const WIERSZ_ZAPAS: f32 = 6.0;

fn uklad_tresci(t: egui::RichText, ui: &egui::Ui, szer: f32) -> egui::text::LayoutJob {
    let mut job = std::sync::Arc::unwrap_or_clone(egui::WidgetText::from(t).into_layout_job(
        ui.style(),
        egui::FontSelection::Default,
        egui::Align::Center,
    ));
    job.wrap.max_width = szer.max(20.0);
    job.wrap.max_rows = TRESC_LINII;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    job
}

/// Wysokość wiersza: treść zawinięta na szerokość kolumny, 1-3 linijki.
fn wysokosc_wiersza(ui: &egui::Ui, w: &Wpis, lista: &[Odbiorca], szer: f32) -> f32 {
    let job = uklad_tresci(tekst_komorki(Kol::Tresc, w, lista, None), ui, szer);
    let g = ui.fonts_mut(|f| f.layout_job(job));
    (g.size().y + WIERSZ_ZAPAS).max(WIERSZ_MIN)
}

/// Komórka treści: zawijana do 3 linijek; zapamiętuje szerokość kolumny do liczenia wysokości wierszy.
fn komorka_tresci(row: &mut TableRow<'_, '_>, t: egui::RichText, tlo: Option<egui::Color32>, id_szer: egui::Id) {
    row.col(|ui| {
        if let Some(c) = tlo {
            ui.painter().rect_filled(ui.max_rect(), 0.0, c);
        }
        krawedzie(ui);
        ui.add_space(5.0);
        let szer = ui.available_width();
        ui.ctx().data_mut(|d| d.insert_temp(id_szer, szer));
        let job = uklad_tresci(t, ui, szer);
        ui.add(egui::Label::new(job).selectable(false).wrap());
    });
}

/// Tekst komórki danej kolumny.
fn tekst_komorki(k: Kol, w: &Wpis, lista: &[Odbiorca], dm: Option<DoMnie>) -> egui::RichText {
    match k {
        Kol::Godzina => egui::RichText::new(&w.godzina),
        Kol::Kierunek if w.nadane => egui::RichText::new("Nadane").color(TAB_NADANE),
        Kol::Kierunek => egui::RichText::new("Odebrane").color(TAB_ODEBRANE),
        Kol::Odbiorca => match (odbiorcy::nazwa(lista, w.ric, w.funkcja), dm) {
            (Some(n), _) => egui::RichText::new(n),
            (None, Some(d)) => egui::RichText::new(d.opis()).color(egui::Color32::BLACK),
            (None, None) => egui::RichText::new(""),
        },
        Kol::Nadawca => match koder::nadawca(&w.tresc).0 {
            // Nazwa z listy odbiorców po samym RIC, inaczej numer.
            Some(r) => egui::RichText::new(odbiorcy::nazwa(lista, r, u8::MAX).map_or_else(|| r.to_string(), str::to_string)),
            None => egui::RichText::new(""),
        },
        Kol::Tresc => egui::RichText::new(koder::nadawca(&w.tresc).1).color(egui::Color32::BLACK),
        Kol::Ric => egui::RichText::new(w.ric.to_string()),
        Kol::Funkcja => egui::RichText::new(funkcja_opis(w.funkcja)),
        Kol::Predkosc => egui::RichText::new(w.predkosc.to_string()),
        Kol::Stan => {
            let (st, kolor) = stan(w);
            egui::RichText::new(st).color(kolor)
        }
    }
}

/// Nagłówek kolumny: przeciągnięty na inny nagłówek przenosi kolumnę w to miejsce.
fn naglowek(ui: &mut egui::Ui, i: usize, k: Kol, przenies: &mut Option<(usize, usize)>, domyslne: &mut bool) {
    let rect = ui.max_rect();
    let resp = ui.interact(rect, ui.scope_id().with(("naglowek", i)), egui::Sense::click_and_drag());
    let cel = resp.dnd_hover_payload::<PrzeciaganaKolumna>().map(|p| p.0).filter(|&z| z != i);
    ui.painter().rect_filled(rect, 0.0, if cel.is_some() { TAB_NAJAZD } else { TAB_NAGLOWEK });
    krawedzie(ui);
    // Kreska: tu wyląduje kolumna (przed tą, gdy idzie z prawej; za nią, gdy z lewej).
    if let Some(z) = cel {
        let x = if z > i { rect.left() + 1.5 } else { rect.right() - 1.5 };
        ui.painter().vline(x, rect.y_range(), egui::Stroke::new(3.0, TAB_NADANE));
    }
    ui.add_space(5.0);
    ui.add(egui::Label::new(egui::RichText::new(k.naglowek()).color(TAB_TEKST)).selectable(false));
    resp.dnd_set_drag_payload(PrzeciaganaKolumna(i));
    if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(poz) = ui.ctx().pointer_latest_pos() {
            let warstwa = egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("przeciagana_kolumna"));
            let p = ui.ctx().layer_painter(warstwa);
            let tekst = p.layout_no_wrap(k.naglowek().to_string(), egui::FontId::proportional(14.0), TAB_TEKST);
            let r = egui::Rect::from_min_size(poz + egui::vec2(12.0, 10.0), tekst.size() + egui::vec2(12.0, 6.0));
            p.rect_filled(r, 3.0, TAB_NAJAZD);
            p.rect_stroke(r, 3.0, egui::Stroke::new(1.0, TAB_NADANE), egui::StrokeKind::Inside);
            p.galley(r.min + egui::vec2(6.0, 3.0), tekst, TAB_TEKST);
        }
    } else if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if let Some(z) = resp.dnd_release_payload::<PrzeciaganaKolumna>().map(|p| p.0).filter(|&z| z != i) {
        *przenies = Some((z, i));
    }
    let resp = resp.on_hover_text("Przeciągnij nagłówek, żeby przesunąć kolumnę; prawy przycisk: kolejność domyślna");
    resp.context_menu(|ui| {
        if ui.button("Przywróć domyślną kolejność kolumn").clicked() {
            *domyslne = true;
            ui.close();
        }
    });
}

/// Historia jako arkusz: białe tło, siatka, nagłówek; nowe na górze. Kliknięty wiersz zwraca
/// RIC i funkcję do konsoli. Kolumny przesuwa się, przeciągając nagłówki.
fn tabela_historii(
    ui: &mut egui::Ui,
    h: &[Wpis],
    lista: &[Odbiorca],
    moje: &MojeRic,
    kolumny: &mut Vec<Kol>,
    klik: &mut Option<(u32, u8)>,
) {
    let v = ui.visuals_mut();
    v.override_text_color = Some(TAB_TEKST);
    v.faint_bg_color = TAB_PASEK;
    v.widgets.hovered.bg_fill = TAB_NAJAZD;
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    let n = h.len();
    let (mut przenies, mut domyslne) = (None, false);
    // Szerokości tabela pamięta według pozycji, więc każda kolejność ma własny stan
    // (po przesunięciu kolumny wracają do swoich szerokości startowych).
    let klucz: String = kolumny.iter().map(|k| k.klucz()).collect::<Vec<_>>().join(",");
    // Wysokość każdego wiersza z liczby linijek treści (1-3) przy szerokości kolumny z poprzedniej klatki.
    let id_szer = ui.scope_id().with(("historia_szer_tresci", &klucz));
    let szer: f32 = ui.ctx().data(|d| d.get_temp(id_szer)).unwrap_or(300.0);
    let z_trescia = kolumny.contains(&Kol::Tresc);
    let wysokosci: Vec<f32> =
        (0..n).map(|i| if z_trescia { wysokosc_wiersza(ui, &h[n - 1 - i], lista, szer) } else { WIERSZ_MIN }).collect();
    let mut tabela = TableBuilder::new(ui)
        .id_salt(("historia", klucz))
        .striped(true)
        .resizable(true)
        .auto_shrink(false)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
    for (i, k) in kolumny.iter().enumerate() {
        tabela = tabela.column(k.kolumna(i + 1 == kolumny.len()));
    }
    tabela
        .header(24.0, |mut r| {
            for (i, &k) in kolumny.iter().enumerate() {
                r.col(|ui| naglowek(ui, i, k, &mut przenies, &mut domyslne));
            }
        })
        .body(|body| {
            body.heterogeneous_rows(wysokosci.into_iter(), |mut row| {
                let w = &h[n - 1 - row.index()];
                let dm = do_mnie(w, moje);
                for &k in kolumny.iter() {
                    if k == Kol::Tresc {
                        komorka_tresci(&mut row, tekst_komorki(k, w, lista, dm), dm.map(DoMnie::tlo), id_szer);
                    } else {
                        komorka(&mut row, tekst_komorki(k, w, lista, dm), dm.map(DoMnie::tlo));
                    }
                }
                let r = row.response();
                if r.clicked() {
                    *klik = Some((w.ric, w.funkcja));
                }
                let r = r.on_hover_text(dymek(w, dm));
                r.context_menu(|ui| {
                    if ui
                        .button("Kopiuj wiersz")
                        .on_hover_text("Rozdzielone tabulatorami — do wklejenia w arkuszu")
                        .clicked()
                    {
                        ui.ctx().copy_text(tekst_wiersza(kolumny, w, lista, dm));
                        ui.close();
                    }
                    if ui.button("Kopiuj treść").clicked() {
                        ui.ctx().copy_text(wartosc_komorki(Kol::Tresc, w, lista, dm));
                        ui.close();
                    }
                    if ui.button("Kopiuj RIC").clicked() {
                        ui.ctx().copy_text(w.ric.to_string());
                        ui.close();
                    }
                });
            });
        });
    if domyslne {
        *kolumny = Kol::DOMYSLNE.to_vec();
    } else if let Some((z, d)) = przenies {
        let k = kolumny.remove(z);
        kolumny.insert(d, k);
    }
}

fn wybor(ui: &mut egui::Ui, etykieta: &str, opcje: &[String], wartosc: &mut String, wlaczony: bool) {
    ui.label(etykieta);
    ui.add_enabled_ui(wlaczony, |ui| {
        egui::ComboBox::from_id_salt(etykieta)
            .selected_text(wartosc.as_str())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for o in opcje {
                    ui.selectable_value(wartosc, o.clone(), o);
                }
            });
    });
    ui.end_row();
}

/// Lista portów COM z opisem sterownika (np. „COM4 — Silicon Labs ... Enhanced COM Port”).
fn wybor_portu(ui: &mut egui::Ui, id: &str, porty: &[(String, String)], port: &mut String) {
    let wybrany = porty.iter().find(|(n, _)| n == port).map_or_else(|| port.clone(), |(_, e)| e.clone());
    egui::ComboBox::from_id_salt(id)
        .selected_text(if wybrany.is_empty() { "wybierz port".to_string() } else { wybrany })
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            if porty.is_empty() {
                ui.label("Brak portów COM");
            }
            for (n, e) in porty {
                ui.selectable_value(port, n.clone(), e);
            }
        });
}

impl eframe::App for Aplikacja {
    fn ui(&mut self, ui: &mut egui::Ui, _ramka: &mut eframe::Frame) {
        ui.ctx().request_repaint_after(Duration::from_millis(100));
        // Zamknięcie okna: najpierw silnik, żeby PTT na pewno puścił.
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            self.silnik = None;
        }
        if self.u != self.zapisane {
            self.u.zapisz();
            self.zapisane = self.u.clone();
        }
        if self.odbiorcy != self.odbiorcy_zapisani {
            self.blad_odbiorcow = odbiorcy::zapisz(&self.odbiorcy).err().map(|e| format!("odbiorcy.txt: {e}"));
            self.odbiorcy_zapisani = self.odbiorcy.clone();
        }
        if self.czestotliwosci != self.czestotliwosci_zapisane {
            self.blad_czest = czestotliwosci::zapisz(&self.czestotliwosci).err().map(|e| format!("czestotliwosci.txt: {e}"));
            self.czestotliwosci_zapisane = self.czestotliwosci.clone();
        }
        egui::CentralPanel::default().show(ui, |ui| {
            egui::CollapsingHeader::new(egui::RichText::new("Ustawienia").strong())
                .default_open(true)
                .show(ui, |ui| self.ustawienia(ui));
            ui.add_space(4.0);
            let dziala = self.silnik.is_some();
            if ui.button(if dziala { "Zatrzymaj" } else { "Start" }).clicked() {
                self.przelacz();
            }
            if let Some(b) = &self.blad_startu {
                ui.colored_label(CZERWONY, b);
            }
            ui.separator();
            self.status(ui);
            ui.separator();
            self.radio(ui);
            ui.separator();
            self.konsola(ui);
            ui.separator();
            self.historia(ui);
        });
        let ctx = ui.ctx().clone();
        self.okno_odbiorcow(&ctx);
        self.okno_czestotliwosci(&ctx);
        self.nowe_do_mnie(&ctx);
    }
}

fn main() -> eframe::Result {
    let ikona = egui::IconData {
        rgba: include_bytes!("../assets/ikona_64.rgba").to_vec(),
        width: 64,
        height: 64,
    };
    let opcje = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 940.0]).with_icon(ikona),
        ..Default::default()
    };
    eframe::run_native("koder_dekoder_pogsag", opcje, Box::new(|_| Ok(Box::new(Aplikacja::new()))))
}

#[cfg(test)]
mod testy {
    use super::*;

    fn wpis(ric: u32) -> Wpis {
        Wpis {
            godzina: "12:00:00".into(),
            nadane: true,
            ric,
            funkcja: 3,
            predkosc: 512,
            tresc: "test".into(),
            druga: String::new(),
            szczegoly: "tekst, nadane".into(),
            uwaga: false,
            zrodlo: String::new(),
            id: 0,
        }
    }

    #[test]
    fn kopiowanie_wiersza_i_tabeli() {
        let kolumny = [Kol::Godzina, Kol::Ric, Kol::Tresc];
        let w = Wpis { godzina: "12:00:00".into(), ric: 123454, tresc: "PUNKT-070\t0706".into(), ..wpis(123454) };
        // Tabulator w treści zostaje: nie jest tu rozdzielaczem kolumn, więc liczba kolumn po
        // wklejeniu w arkuszu by się zgadzała tylko wtedy, gdyby treść go nie zawierała — program
        // i tak nigdy go tam nie wstawia (ani koder, ani dekoder).
        assert_eq!(tekst_wiersza(&kolumny, &w, &[], None), "12:00:00\t123454\tPUNKT-070\t0706");

        let h = vec![wpis(100), w];
        let t = tekst_tabeli(&kolumny, &h, &[], &MojeRic { wlasny: 0, wlasny_wl: false, nadawca_wl: false, grupowy: 0, grupowy_wl: false });
        let wiersze: Vec<&str> = t.split("\r\n").collect();
        assert_eq!(wiersze[0], "Godzina\tRIC\tTreść");
        // Nowe na górze, jak w tabeli.
        assert_eq!(wiersze[1], "12:00:00\t123454\tPUNKT-070\t0706");
        assert_eq!(wiersze[2], "12:00:00\t100\ttest");
        assert_eq!(wiersze.len(), 3);
    }

    /// Użytkownik 07.11: „jeden wiersz mógł zwiększać się na 3 linijki”.
    #[test]
    fn wiersz_do_trzech_linijek() {
        let ctx = egui::Context::default();
        let wej = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0))),
            ..Default::default()
        };
        let _ = ctx.run_ui(wej, |ui| {
            let mut w = wpis(1);
            let krotki = wysokosc_wiersza(ui, &w, &[], 300.0);
            w.tresc = "SLOWO ".repeat(8);
            let dwie = wysokosc_wiersza(ui, &w, &[], 300.0);
            w.tresc = "SLOWO ".repeat(200);
            let dluga = wysokosc_wiersza(ui, &w, &[], 300.0);
            w.tresc = "SLOWO ".repeat(400);
            let bardzo = wysokosc_wiersza(ui, &w, &[], 300.0);
            assert_eq!(krotki, WIERSZ_MIN, "krótka treść: jedna linijka jak dawniej");
            assert!(dwie > krotki, "dłuższa treść: wyższy wiersz ({dwie} vs {krotki})");
            assert!(dluga > dwie && dluga < krotki * 3.5, "najwyżej 3 linijki ({dluga})");
            assert_eq!(dluga, bardzo, "więcej niż 3 linijki: dalej 3 (reszta za „…”)");
            assert!(wysokosc_wiersza(ui, &w, &[], 900.0) <= dluga, "szersza kolumna: nie wyżej");
        });
    }

    /// Klatki okna bez ekranu: wciśnięcie na nagłówku, przeciągnięcie, puszczenie.
    fn przeciagnij(kolumny: &mut Vec<Kol>, z: egui::Pos2, do_: egui::Pos2) {
        let ctx = egui::Context::default();
        let h = vec![wpis(1), wpis(2), wpis(3)];
        let przycisk = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut klatki: Vec<Vec<egui::Event>> =
            vec![vec![], vec![egui::Event::PointerMoved(z)], vec![przycisk(z, true)]];
        for k in 1..=10 {
            klatki.push(vec![egui::Event::PointerMoved(z + (do_ - z) * (k as f32 / 10.0))]);
        }
        klatki.push(vec![przycisk(do_, false)]);
        klatki.push(vec![]);
        for zdarzenia in klatki {
            let wej = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0))),
                events: zdarzenia,
                ..Default::default()
            };
            let _ = ctx.run_ui(wej, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let mut klik = None;
                    let moje = MojeRic { wlasny: 0, wlasny_wl: false, nadawca_wl: false, grupowy: 0, grupowy_wl: false };
                    tabela_historii(ui, &h, &[], &moje, kolumny, &mut klik);
                });
            });
        }
    }

    #[test]
    fn przeciagniecie_naglowka_przesuwa_kolumne() {
        // Nagłówek na y 8-32 (margines panelu 8); kolumny bez odstępów: Godzina 8-76, Kierunek
        // 76-156, Nadawca 156-266, Odbiorca 266-396, Treść 396-696, RIC 696-770.
        let mut k = Kol::DOMYSLNE.to_vec();
        przeciagnij(&mut k, egui::pos2(546.0, 20.0), egui::pos2(40.0, 20.0));
        assert_eq!(k[..5], [Kol::Tresc, Kol::Godzina, Kol::Kierunek, Kol::Nadawca, Kol::Odbiorca]);
        // RIC (nadal 696-770) w prawo, na Stan (ostatnia): ląduje na końcu.
        przeciagnij(&mut k, egui::pos2(733.0, 20.0), egui::pos2(990.0, 20.0));
        assert_eq!(*k.last().unwrap(), Kol::Ric);
        assert_eq!(k.len(), 9);
    }

    #[test]
    fn krawedz_kolumny_zmienia_szerokosc_a_nie_kolejnosc() {
        // Granica Odbiorca | Treść na x = 396: tam jest uchwyt szerokości tabeli.
        let mut k = Kol::DOMYSLNE.to_vec();
        przeciagnij(&mut k, egui::pos2(396.0, 20.0), egui::pos2(450.0, 20.0));
        assert_eq!(k, Kol::DOMYSLNE.to_vec());
    }

    #[test]
    fn wiadomosc_do_mnie_tylko_po_ric() {
        let m = MojeRic { wlasny: 100, wlasny_wl: true, nadawca_wl: true, grupowy: 200, grupowy_wl: true };
        let odebrany = |ric, funkcja| Wpis { nadane: false, funkcja, ..wpis(ric) };
        assert_eq!(do_mnie(&odebrany(100, 0), &m), Some(DoMnie::Wlasny));
        assert_eq!(do_mnie(&odebrany(100, 3), &m), Some(DoMnie::Wlasny), "funkcja bez znaczenia");
        assert_eq!(do_mnie(&odebrany(200, 2), &m), Some(DoMnie::Grupa));
        assert_eq!(do_mnie(&odebrany(300, 0), &m), None);
        assert_eq!(do_mnie(&wpis(100), &m), None, "własne nadanie to nie odbiór");
        let wyl = MojeRic { wlasny_wl: false, grupowy_wl: false, ..m };
        assert_eq!(do_mnie(&odebrany(100, 0), &wyl), None, "wyłączone");
    }

    #[test]
    fn kolejnosc_kolumn_w_ustawieniach() {
        // Układ zapisany przed kolumną Nadawca: zostaje, a Nadawca staje przed Odbiorcą.
        let t = "tresc,godzina,kierunek,odbiorca,ric,funkcja,predkosc,stan";
        let k = kolumny_z_tekstu(t).unwrap();
        assert_eq!(k[..5], [Kol::Tresc, Kol::Godzina, Kol::Kierunek, Kol::Nadawca, Kol::Odbiorca]);
        assert_eq!(k.len(), 9);
        assert_eq!(kolumny_z_tekstu("tresc,godzina").unwrap().len(), 9, "brakujące dopisane");
        assert!(kolumny_z_tekstu("tresc,tresc,kierunek").is_none(), "powtórka");
        assert!(kolumny_z_tekstu("tresc,cos").is_none(), "nieznana kolumna");
    }
}
