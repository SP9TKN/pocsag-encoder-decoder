//! Praca na żywo: nasłuch (wejście karty -> dekoder POCSAG) i nadawanie (koder -> wyjście karty
//! z kluczowaniem PTT). Nadajnik ma własny wątek i kolejkę; wiadomości z kolejki o tych samych
//! ustawieniach idą jednym nadaniem, z jedną preambułą.

use crate::dziennik::{self, Wpis};
use crate::koder::{self, DoNadania, Modulacja, Rodzaj};
use crate::korekcja::{self, Korekcja};
use crate::pocsag::{DekoderNaZywo, Wiadomosc};
use crate::ptt::{self, Ptt, UstawieniaCat, UstawieniaPtt};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, ErrorKind, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const BEZ_NASLUCHU: &str = "— bez nasłuchu —";
/// Co tyle sekund okno POCSAG jest dekodowane od nowa (ok. 75 ms pracy na okno).
const POCSAG_CO_S: f32 = 1.0;
const MAX_WPISOW: usize = 2000;
/// Tyle wiadomości z kolejki idzie jednym nadaniem.
const MAX_W_NADANIU: usize = 8;
/// Odbiór tej samej treści na ten sam RIC tak szybko po nadaniu to własne nadanie.
const ECHO_S: f32 = 120.0;
/// Karta gra jeszcze chwilę po ostatniej próbce oddanej do bufora.
const ZAPAS_KARTY_MS: u64 = 60;
/// Strumień wyjścia rusza przed PTT, żeby start karty nie zjadł początku preambuły.
const ROZRUCH_KARTY_MS: u64 = 80;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Parametry {
    pub predkosc: u32,
    /// Amplituda wyjścia 0..1.
    pub poziom: f32,
    pub odwroc: bool,
    /// Ile razy całe nadanie (z preambułą) idzie pod jednym PTT.
    pub powtorzenia: u32,
    /// Od włączenia PTT do początku danych: nadajnik i pager potrzebują chwili.
    pub opoznienie_ms: u32,
    /// Cisza po danych, zanim PTT puści.
    pub ogon_ms: u32,
    /// Odwrotność charakterystyki wejścia audio nadajnika (np. preemfaza IC-7100 przez USB).
    pub korekcja: Korekcja,
}

pub enum Zlecenie {
    Wiadomosc {
        w: DoNadania,
        /// Tekst, jak go wpisano (przed zamianą ogonków), do historii.
        wpisane: String,
        par: Parametry,
    },
    /// Sama preambuła, do ustawienia poziomu i dewiacji.
    Probny { sekundy: f32, par: Parametry },
    /// Częstotliwość radia przez CAT (między nadaniami, nigdy w trakcie).
    Czestotliwosc { hz: u64 },
}

/// Wiadomość w eterze w tej sesji, do rozpoznania własnego nadania w odbiorze.
struct Nadane {
    ric: u32,
    predkosc: u32,
    tresc: String,
    sygnal: bool,
    chwila: Instant,
}

#[derive(Default, Clone)]
pub struct StanTx {
    pub nadaje: bool,
    /// Co jest nadawane albo wynik ostatniego nadania.
    pub opis: String,
}

pub struct Wspolne {
    pub historia: Mutex<Vec<Wpis>>,
    pub blad: Mutex<Option<String>>,
    pub tx: Mutex<StanTx>,
    poziom: AtomicU32,
    postep: AtomicU32,
    kolejka: Mutex<VecDeque<Zlecenie>>,
    sygnal: Condvar,
    przerwij: AtomicBool,
    /// Wpisywane na starcie nadania: dekoder potrafi oddać własną wiadomość, zanim nadanie się skończy.
    nadawane: Mutex<Vec<Nadane>>,
    nastepny_id: AtomicU64,
    /// Dziury w nagraniu wejścia (znaczniki czasu karty): ile razy i ile ms razem.
    luki: AtomicU32,
    luki_ms: AtomicU32,
    /// Wynik ostatniej zmiany częstotliwości: tekst i czy to błąd.
    pub radio: Mutex<Option<(String, bool)>>,
}

impl Wspolne {
    /// Startuje z historią z dzisiejszego pliku.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            historia: Mutex::new(dziennik::wczytaj_dzis()),
            blad: Mutex::default(),
            tx: Mutex::default(),
            poziom: AtomicU32::new(0),
            postep: AtomicU32::new(0),
            kolejka: Mutex::default(),
            sygnal: Condvar::new(),
            przerwij: AtomicBool::new(false),
            nadawane: Mutex::default(),
            nastepny_id: AtomicU64::new(1),
            luki: AtomicU32::new(0),
            luki_ms: AtomicU32::new(0),
            radio: Mutex::default(),
        })
    }

    /// Wynik zmiany częstotliwości do okna.
    pub fn wynik_radia(&self, hz: u64, w: Result<(), String>) {
        *self.radio.lock().unwrap() = Some(match w {
            Ok(()) => (format!("radio: {} MHz", ptt::tekst_czest(hz)), false),
            Err(e) => (format!("radio: {e}"), true),
        });
    }

    /// Częstotliwość radia: przez wątek nadajnika, gdy silnik działa (port PTT bywa wspólny z CAT),
    /// inaczej od razu, w osobnym wątku, z portem CAT otwieranym na chwilę.
    pub fn ustaw_czestotliwosc(self: &Arc<Self>, dziala: bool, cat: &UstawieniaCat, hz: u64) {
        *self.radio.lock().unwrap() = Some((format!("radio: ustawiam {} MHz...", ptt::tekst_czest(hz)), false));
        if dziala {
            self.zlec(Zlecenie::Czestotliwosc { hz });
        } else {
            // Szybkie kliknięcia po kolei: dwa wątki nie otworzą portu naraz.
            static ZAMEK: Mutex<()> = Mutex::new(());
            let (w, c) = (self.clone(), cat.clone());
            std::thread::spawn(move || {
                let _z = ZAMEK.lock().unwrap_or_else(|e| e.into_inner());
                w.wynik_radia(hz, ptt::czestotliwosc_osobno(&c, hz))
            });
        }
    }

    pub fn dodaj_wpis(&self, w: Wpis) {
        let mut h = self.historia.lock().unwrap();
        h.push(w);
        if h.len() > MAX_WPISOW {
            h.remove(0);
        }
    }

    /// Wpis do okna i do pliku dnia; błąd zapisu idzie na czerwono do okna.
    fn zapisz_wpis(&self, data: &str, w: Wpis) {
        self.do_pliku(data, &w);
        self.dodaj_wpis(w);
    }

    fn do_pliku(&self, data: &str, w: &Wpis) {
        if let Err(e) = dziennik::zapisz(data, w) {
            *self.blad.lock().unwrap() = Some(format!("plik historii: {e}"));
        }
    }

    /// Wynik nadania do wpisu, który już jest w oknie (o ile nie wyczyszczono go w międzyczasie).
    fn uzupelnij(&self, w: &Wpis) {
        if let Some(h) = self.historia.lock().unwrap().iter_mut().rev().find(|h| h.id == w.id) {
            *h = w.clone();
        }
    }

    pub fn zlec(&self, z: Zlecenie) {
        self.kolejka.lock().unwrap().push_back(z);
        self.sygnal.notify_all();
    }

    pub fn w_kolejce(&self) -> usize {
        self.kolejka.lock().unwrap().len()
    }

    /// Przerywa bieżące nadanie i czyści kolejkę; zwraca liczbę usuniętych zleceń.
    pub fn przerwij(&self) -> usize {
        let mut q = self.kolejka.lock().unwrap();
        let n = q.len();
        if q.iter().any(|z| matches!(z, Zlecenie::Czestotliwosc { .. })) {
            *self.radio.lock().unwrap() = Some(("radio: zmiana częstotliwości przerwana".into(), true));
        }
        q.clear();
        self.przerwij.store(true, Ordering::Relaxed);
        n
    }

    pub fn poziom(&self) -> f32 {
        f32::from_bits(self.poziom.load(Ordering::Relaxed))
    }

    pub fn postep(&self) -> f32 {
        f32::from_bits(self.postep.load(Ordering::Relaxed))
    }

    /// Ile razy wejście zgubiło próbki i ile ms razem (od Startu).
    pub fn luki(&self) -> (u32, u32) {
        (self.luki.load(Ordering::Relaxed), self.luki_ms.load(Ordering::Relaxed))
    }
}

// ---------- urządzenia ----------

pub fn nazwa(d: &Device) -> Option<String> {
    d.description().ok().map(|o| o.name().to_string())
}

pub fn urzadzenia_wejsciowe() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices().map_or(Vec::new(), |l| l.filter_map(|d| nazwa(&d)).collect())
}

pub fn urzadzenia_wyjsciowe() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices().map_or(Vec::new(), |l| l.filter_map(|d| nazwa(&d)).collect())
}

pub fn domyslne() -> (Option<String>, Option<String>) {
    let host = cpal::default_host();
    (
        host.default_input_device().and_then(|d| nazwa(&d)),
        host.default_output_device().and_then(|d| nazwa(&d)),
    )
}

/// Najpierw pełna nazwa, potem fragment (np. „CABLE-B” z linii poleceń).
fn znajdz(lista: impl Iterator<Item = Device>, nazwa_urz: &str) -> Option<Device> {
    let l: Vec<Device> = lista.collect();
    let i = l
        .iter()
        .position(|d| nazwa(d).as_deref() == Some(nazwa_urz))
        .or_else(|| l.iter().position(|d| nazwa(d).is_some_and(|n| n.contains(nazwa_urz))))?;
    l.into_iter().nth(i)
}

pub fn znajdz_wejscie(n: &str) -> Result<Device, String> {
    let host = cpal::default_host();
    let l = host.input_devices().map_err(|e| e.to_string())?;
    znajdz(l, n).ok_or_else(|| format!("brak wejścia: {n}"))
}

pub fn znajdz_wyjscie(n: &str) -> Result<Device, String> {
    let host = cpal::default_host();
    let l = host.output_devices().map_err(|e| e.to_string())?;
    znajdz(l, n).ok_or_else(|| format!("brak wyjścia: {n}"))
}

/// Xrun i podobne to chwilowe zdarzenia: strumień gra dalej, więc to nie jest awaria.
fn powazny(e: &cpal::Error) -> bool {
    !matches!(e.kind(), ErrorKind::Xrun | ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied)
}

// ---------- nasłuch ----------

struct Wejscie {
    wspolne: Arc<Wspolne>,
    bufor: Arc<Mutex<Vec<f32>>>,
    kanaly: usize,
    szczyt: f32,
    fs: f32,
    /// Znacznik czasu poprzedniego pakietu (ns) i liczba jego ramek.
    poprzedni: Option<(u128, usize)>,
}

/// Pakiet spóźniony o więcej niż to względem poprzedniego to zgubione próbki.
const LUKA_NS: u128 = 4_000_000;

impl Wejscie {
    fn przyjmij<T>(&mut self, dane: &[T], info: &cpal::InputCallbackInfo)
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        // Karta pod obciążeniem gubi czasem cały pakiet (10 ms): wiadomość w tej chwili może
        // zginąć, więc to liczymy i pokazujemy w oknie.
        let teraz = info.timestamp().capture.as_nanos();
        let ramek = dane.len() / self.kanaly;
        if let Some((t0, n0)) = self.poprzedni {
            let oczekiwany = t0 + (n0 as f64 * 1e9 / self.fs as f64) as u128;
            if teraz > oczekiwany + LUKA_NS {
                self.wspolne.luki.fetch_add(1, Ordering::Relaxed);
                self.wspolne.luki_ms.fetch_add(((teraz - oczekiwany) / 1_000_000) as u32, Ordering::Relaxed);
            }
        }
        self.poprzedni = Some((teraz, ramek));
        let mut b = self.bufor.lock().unwrap();
        self.szczyt *= 0.8;
        for r in dane.chunks_exact(self.kanaly) {
            let v = r.iter().map(|&s| f32::from_sample(s)).sum::<f32>() / self.kanaly as f32;
            self.szczyt = self.szczyt.max(v.abs());
            b.push(v);
        }
        self.wspolne.poziom.store(self.szczyt.to_bits(), Ordering::Relaxed);
    }
}

fn strumien_wejscia<T>(d: &Device, cfg: StreamConfig, mut wej: Wejscie, wspolne: Arc<Wspolne>) -> Result<Stream, String>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    d.build_input_stream::<T, _, _>(
        cfg,
        move |dane: &[T], info: &cpal::InputCallbackInfo| wej.przyjmij(dane, info),
        move |e| {
            if powazny(&e) {
                *wspolne.blad.lock().unwrap() = Some(format!("wejście: {e}"));
            }
        },
        None,
    )
    .map_err(|e| format!("wejście: {e}"))
}

/// Wpis odebranej wiadomości; własne nadanie (odsłuch przez radio albo kabel) dostaje dopisek.
fn wpis_odebrany(w: &Wiadomosc, godzina: String, nadawane: &[Nadane]) -> Wpis {
    let (glowna, druga) = w.odczyty();
    // Oba odczyty: tekst z samych cyfr dekoder pokazuje jako cyfry.
    let echo = nadawane.iter().any(|n| {
        n.ric == w.ric
            && n.predkosc == w.predkosc
            && n.chwila.elapsed().as_secs_f32() < ECHO_S
            && if n.sygnal { w.dane.is_empty() } else { w.tekst() == n.tresc || w.cyfry() == n.tresc }
    });
    let mut szczegoly = w.jakosc();
    if echo {
        szczegoly += ", własne nadanie (odsłuch)";
    }
    Wpis {
        godzina,
        nadane: false,
        ric: w.ric,
        funkcja: w.funkcja,
        predkosc: w.predkosc,
        tresc: glowna,
        druga: druga.unwrap_or_default(),
        szczegoly,
        uwaga: w.bledne_slowa > 0 || !w.zakonczona,
        zrodlo: String::new(),
        id: 0,
    }
}

fn start_nasluchu(nazwa_wej: &str, wspolne: &Arc<Wspolne>, stop: &Arc<AtomicBool>) -> Result<Stream, String> {
    let d = znajdz_wejscie(nazwa_wej)?;
    let k = d.default_input_config().map_err(|e| format!("wejście: {e}"))?;
    let fs = k.sample_rate() as f32;
    let bufor = Arc::new(Mutex::new(Vec::new()));
    let wej = Wejscie {
        wspolne: wspolne.clone(),
        bufor: bufor.clone(),
        kanaly: k.channels() as usize,
        szczyt: 0.0,
        fs,
        poprzedni: None,
    };
    wspolne.luki.store(0, Ordering::Relaxed);
    wspolne.luki_ms.store(0, Ordering::Relaxed);
    let cfg = k.config();
    let w = wspolne.clone();
    let s = match k.sample_format() {
        SampleFormat::F32 => strumien_wejscia::<f32>(&d, cfg, wej, w),
        SampleFormat::I16 => strumien_wejscia::<i16>(&d, cfg, wej, w),
        SampleFormat::I32 => strumien_wejscia::<i32>(&d, cfg, wej, w),
        SampleFormat::U8 => strumien_wejscia::<u8>(&d, cfg, wej, w),
        f => Err(format!("wejście: nieobsługiwany format {f}")),
    }?;
    s.play().map_err(|e| format!("wejście: {e}"))?;
    let (wspolne, stop) = (wspolne.clone(), stop.clone());
    // Diagnostyka: POGSAG_ZRZUT=plik.wav zapisuje wszystko, co weszło na wejście.
    let mut zrzut = std::env::var_os("POGSAG_ZRZUT").and_then(|p| {
        let spec = hound::WavSpec { channels: 1, sample_rate: fs as u32, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        hound::WavWriter::create(p, spec).ok()
    });
    std::thread::spawn(move || {
        let mut dek = DekoderNaZywo::new(fs, POCSAG_CO_S);
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
            let probki = std::mem::take(&mut *bufor.lock().unwrap());
            if let Some(z) = zrzut.as_mut() {
                for &v in &probki {
                    let _ = z.write_sample((v.clamp(-1.0, 1.0) * 32767.0) as i16);
                }
            }
            for w in dek.dodaj(&probki) {
                let temu_s = (dek.koniec() - w.probka) as f32 / fs;
                let (data, godzina) = dziennik::data_i_godzina(temu_s);
                let wpis = wpis_odebrany(&w, godzina, &wspolne.nadawane.lock().unwrap());
                wspolne.zapisz_wpis(&data, wpis);
            }
        }
        if let Some(z) = zrzut {
            let _ = z.finalize();
        }
    });
    Ok(s)
}

// ---------- nadawanie ----------

#[derive(Default)]
struct StanOdtwarzania {
    start: AtomicBool,
    koniec: AtomicBool,
    poz: AtomicUsize,
    blad: Mutex<Option<String>>,
}

struct Odtwarzacz {
    probki: Vec<f32>,
    poz: usize,
    kanaly: usize,
    stan: Arc<StanOdtwarzania>,
    wspolne: Arc<Wspolne>,
}

impl Odtwarzacz {
    fn wypelnij<T>(&mut self, dane: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
    {
        // Przed startem (PTT jeszcze nie wciśnięty) i po końcu: cisza.
        let gra = self.stan.start.load(Ordering::Relaxed) && !self.stan.koniec.load(Ordering::Relaxed);
        let przerwane = self.wspolne.przerwij.load(Ordering::Relaxed);
        for ramka in dane.chunks_exact_mut(self.kanaly) {
            let mut y = 0.0;
            if gra && !przerwane && self.poz < self.probki.len() {
                y = self.probki[self.poz];
                self.poz += 1;
            }
            ramka.fill(T::from_sample(y));
        }
        if gra {
            self.stan.poz.store(self.poz, Ordering::Relaxed);
            if przerwane || self.poz >= self.probki.len() {
                self.stan.koniec.store(true, Ordering::Relaxed);
            }
        }
    }
}

fn strumien_wyjscia<T>(d: &Device, cfg: StreamConfig, mut o: Odtwarzacz) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let stan = o.stan.clone();
    d.build_output_stream::<T, _, _>(
        cfg,
        move |dane: &mut [T], _| o.wypelnij(dane),
        move |e| {
            if powazny(&e) {
                *stan.blad.lock().unwrap() = Some(format!("wyjście: {e}"));
                stan.koniec.store(true, Ordering::Relaxed);
            }
        },
        None,
    )
    .map_err(|e| format!("wyjście: {e}"))
}

enum Wynik {
    /// Czas samych danych w sekundach.
    Nadane(f32),
    Przerwane,
    Blad(String),
}

/// Jedno nadanie: strumień rusza, PTT, opóźnienie, dane, ogon, PTT puszcza. PTT puszcza zawsze,
/// także po błędzie karty, po przerwaniu i po limicie czasu.
fn nadaj(wyjscie: &str, bity: &[u8], par: &Parametry, ptt: &mut Ptt, wspolne: &Arc<Wspolne>) -> Wynik {
    let przygotowanie = || -> Result<(Stream, Arc<StanOdtwarzania>, usize, f32, f32), String> {
        let d = znajdz_wyjscie(wyjscie)?;
        let k = d.default_output_config().map_err(|e| format!("wyjście: {e}"))?;
        let fs = k.sample_rate() as f32;
        let cisza = |ms: u32| vec![0.0f32; (ms as f32 / 1000.0 * fs) as usize];
        let sygnal = koder::audio(bity, &Modulacja { predkosc: par.predkosc, fs, poziom: par.poziom, odwroc: par.odwroc });
        let sygnal = korekcja::koryguj(&sygnal, fs, par.korekcja);
        let dane_s = sygnal.len() as f32 / fs;
        let mut probki = cisza(par.opoznienie_ms);
        probki.extend(sygnal);
        probki.extend(cisza(par.ogon_ms));
        let dlugosc = probki.len();
        let stan = Arc::new(StanOdtwarzania::default());
        let o = Odtwarzacz { probki, poz: 0, kanaly: k.channels() as usize, stan: stan.clone(), wspolne: wspolne.clone() };
        let cfg = k.config();
        let s = match k.sample_format() {
            SampleFormat::F32 => strumien_wyjscia::<f32>(&d, cfg, o),
            SampleFormat::I16 => strumien_wyjscia::<i16>(&d, cfg, o),
            SampleFormat::I32 => strumien_wyjscia::<i32>(&d, cfg, o),
            SampleFormat::U8 => strumien_wyjscia::<u8>(&d, cfg, o),
            f => Err(format!("wyjście: nieobsługiwany format {f}")),
        }?;
        s.play().map_err(|e| format!("wyjście: {e}"))?;
        Ok((s, stan, dlugosc, fs, dane_s))
    };
    let (strumien, stan, dlugosc, fs, dane_s) = match przygotowanie() {
        Ok(p) => p,
        Err(e) => return Wynik::Blad(e),
    };
    std::thread::sleep(Duration::from_millis(ROZRUCH_KARTY_MS));
    if wspolne.przerwij.load(Ordering::Relaxed) {
        return Wynik::Przerwane;
    }
    if let Err(e) = ptt.ustaw(true) {
        let _ = ptt.ustaw(false);
        return Wynik::Blad(e);
    }
    stan.start.store(true, Ordering::Relaxed);
    let limit = Instant::now() + Duration::from_secs_f32(dlugosc as f32 / fs + 3.0);
    let mut za_dlugo = false;
    while !stan.koniec.load(Ordering::Relaxed) {
        if Instant::now() > limit {
            za_dlugo = true;
            break;
        }
        let p = stan.poz.load(Ordering::Relaxed) as f32 / dlugosc.max(1) as f32;
        wspolne.postep.store(p.to_bits(), Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(10));
    }
    let przerwane = wspolne.przerwij.load(Ordering::Relaxed);
    let blad = stan.blad.lock().unwrap().clone();
    if !przerwane && !za_dlugo && blad.is_none() {
        std::thread::sleep(Duration::from_millis(ZAPAS_KARTY_MS));
    }
    let puszczenie = ptt.ustaw(false);
    drop(strumien);
    wspolne.postep.store(0f32.to_bits(), Ordering::Relaxed);
    if let Some(e) = blad {
        Wynik::Blad(e)
    } else if za_dlugo {
        Wynik::Blad("wyjście audio nie odtwarza (limit czasu)".into())
    } else if przerwane {
        Wynik::Przerwane
    } else if let Err(e) = puszczenie {
        Wynik::Blad(format!("PTT nie puścił: {e}"))
    } else {
        Wynik::Nadane(dane_s)
    }
}

fn s_pl(x: f32) -> String {
    format!("{x:.1}").replace('.', ",")
}

/// Bierze z kolejki pierwsze zlecenie i następne wiadomości o tych samych ustawieniach.
fn pobierz(wspolne: &Wspolne, stop: &AtomicBool) -> Option<Vec<Zlecenie>> {
    let mut q = wspolne.kolejka.lock().unwrap();
    loop {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        if !q.is_empty() {
            break;
        }
        q = wspolne.sygnal.wait_timeout(q, Duration::from_millis(200)).unwrap().0;
    }
    let mut p = vec![q.pop_front().unwrap()];
    if let Zlecenie::Wiadomosc { par, .. } = &p[0] {
        let par = *par;
        while p.len() < MAX_W_NADANIU && matches!(q.front(), Some(Zlecenie::Wiadomosc { par: p2, .. }) if *p2 == par) {
            p.push(q.pop_front().unwrap());
        }
    }
    Some(p)
}

fn watek_nadajnika(wyjscie: String, mut ptt: Ptt, opis_ptt: String, wspolne: Arc<Wspolne>, stop: Arc<AtomicBool>) {
    while let Some(paczka) = pobierz(&wspolne, &stop) {
        wspolne.przerwij.store(false, Ordering::Relaxed);
        let wiadomosci: Vec<(&DoNadania, &str)> = paczka
            .iter()
            .filter_map(|z| match z {
                Zlecenie::Wiadomosc { w, wpisane, .. } => Some((w, wpisane.as_str())),
                Zlecenie::Probny { .. } | Zlecenie::Czestotliwosc { .. } => None,
            })
            .collect();
        let (bity, par) = match &paczka[0] {
            Zlecenie::Czestotliwosc { hz } => {
                let w = ptt.czestotliwosc(*hz);
                wspolne.wynik_radia(*hz, w);
                continue;
            }
            Zlecenie::Probny { sekundy, par } => (koder::bity_probne(par.predkosc, *sekundy), *par),
            Zlecenie::Wiadomosc { par, .. } => {
                let lista: Vec<DoNadania> = wiadomosci.iter().map(|(w, _)| (*w).clone()).collect();
                let jedno = koder::bity(&lista);
                let mut b = Vec::with_capacity(jedno.len() * par.powtorzenia.max(1) as usize);
                for _ in 0..par.powtorzenia.max(1) {
                    b.extend_from_slice(&jedno);
                }
                (b, *par)
            }
        };
        let czas = koder::czas_s(bity.len(), par.predkosc);
        let co = if wiadomosci.is_empty() {
            "ton próbny".to_string()
        } else if wiadomosci.len() == 1 {
            format!("RIC {}", wiadomosci[0].0.ric)
        } else {
            format!("{} wiadomości", wiadomosci.len())
        };
        *wspolne.tx.lock().unwrap() =
            StanTx { nadaje: true, opis: format!("Nadaję: {co}, {} bit/s, {} s", par.predkosc, s_pl(czas)) };
        {
            let mut n = wspolne.nadawane.lock().unwrap();
            n.retain(|x| x.chwila.elapsed().as_secs_f32() < ECHO_S);
            n.extend(wiadomosci.iter().map(|(w, _)| Nadane {
                ric: w.ric,
                predkosc: par.predkosc,
                tresc: w.tresc.clone(),
                sygnal: w.rodzaj == Rodzaj::Sygnal,
                chwila: Instant::now(),
            }));
        }
        // Wpisy od razu w oknie, nad nimi pojawi się własny odbiór; do pliku po wyniku.
        let (data, godzina) = dziennik::data_i_godzina(0.0);
        let mut wpisy: Vec<Wpis> = wiadomosci
            .iter()
            .map(|(w, wpisane)| Wpis {
                godzina: godzina.clone(),
                nadane: true,
                ric: w.ric,
                funkcja: w.funkcja,
                predkosc: par.predkosc,
                tresc: if w.rodzaj == Rodzaj::Sygnal { "(sam sygnał, bez treści)".to_string() } else { w.tresc.clone() },
                druga: if w.rodzaj == Rodzaj::Tekst && *wpisane != w.tresc { format!("wpisane: {wpisane}") } else { String::new() },
                szczegoly: format!("{}, nadaję...", w.rodzaj.nazwa()),
                uwaga: false,
                zrodlo: String::new(),
                id: wspolne.nastepny_id.fetch_add(1, Ordering::Relaxed),
            })
            .collect();
        for w in &wpisy {
            wspolne.dodaj_wpis(w.clone());
        }
        let wynik = nadaj(&wyjscie, &bity, &par, &mut ptt, &wspolne);
        let (opis, uwaga) = match &wynik {
            Wynik::Nadane(s) => {
                let mut o = format!("nadane {} s, PTT {}, poziom {:.0}%", s_pl(*s), opis_ptt, par.poziom * 100.0);
                if par.powtorzenia > 1 {
                    o += &format!(", {}× pod jednym PTT", par.powtorzenia);
                }
                if par.odwroc {
                    o += ", polaryzacja odwrócona";
                }
                (o, false)
            }
            Wynik::Przerwane => ("PRZERWANE".to_string(), true),
            Wynik::Blad(e) => (format!("BŁĄD: {e}"), true),
        };
        for (wpis, (w, _)) in wpisy.iter_mut().zip(&wiadomosci) {
            wpis.szczegoly = format!("{}, {opis}", w.rodzaj.nazwa());
            wpis.uwaga = uwaga;
            wspolne.uzupelnij(wpis);
            wspolne.do_pliku(&data, wpis);
        }
        let koniec = match &wynik {
            Wynik::Nadane(_) if wiadomosci.is_empty() => "Ton próbny nadany".to_string(),
            Wynik::Nadane(_) => format!("Nadane: {co}"),
            _ => format!("{co}: {opis}"),
        };
        *wspolne.tx.lock().unwrap() = StanTx { nadaje: false, opis: koniec };
    }
    // Tu `ptt` znika: Drop puszcza PTT, zanim port się zamknie.
}

pub struct Silnik {
    _wejscie: Option<Stream>,
    stop: Arc<AtomicBool>,
    nadajnik: Option<JoinHandle<()>>,
    wspolne: Arc<Wspolne>,
    pub nasluch: bool,
}

impl Drop for Silnik {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.wspolne.przerwij();
        self.wspolne.sygnal.notify_all();
        // Czekamy na wątek nadajnika: PTT ma puścić, zanim program się zamknie.
        if let Some(h) = self.nadajnik.take() {
            let _ = h.join();
        }
        self.wspolne.poziom.store(0, Ordering::Relaxed);
    }
}

impl Silnik {
    pub fn start(
        wejscie: &str,
        wyjscie: &str,
        ust_ptt: &UstawieniaPtt,
        cat: &UstawieniaCat,
        wspolne: Arc<Wspolne>,
    ) -> Result<Self, String> {
        *wspolne.blad.lock().unwrap() = None;
        // Wyjście sprawdzone od razu, a nie dopiero przy pierwszym nadaniu.
        znajdz_wyjscie(wyjscie)?.default_output_config().map_err(|e| format!("wyjście: {e}"))?;
        let ptt = Ptt::otworz(ust_ptt, cat)?;
        let stop = Arc::new(AtomicBool::new(false));
        let wejscie_s = if wejscie == BEZ_NASLUCHU { None } else { Some(start_nasluchu(wejscie, &wspolne, &stop)?) };
        wspolne.przerwij.store(false, Ordering::Relaxed);
        let (wy, op, w, s) = (wyjscie.to_string(), ust_ptt.opis(), wspolne.clone(), stop.clone());
        let nadajnik = std::thread::spawn(move || watek_nadajnika(wy, ptt, op, w, s));
        Ok(Self { nasluch: wejscie_s.is_some(), _wejscie: wejscie_s, stop, nadajnik: Some(nadajnik), wspolne })
    }
}
