// RayDrone — motor granular continuo en Rust (no_std, sin dependencias).
//
// Cada grano es un "rayo" lanzado a un offset aleatorio dentro del cono de dispersión
// alrededor del foco. Se mezclan muestra a muestra (sin nodos, sin tope de voces, sin
// jitter). Incluye: muestreo random/stratified/QMC, aberración cromática por bandas,
// rebotes (Russian roulette), autoevolución recursiva, paneo estéreo y octava (shimmer).
//
// Compilar (ver build.sh):
//   rustc --edition 2021 --target wasm32-unknown-unknown -O -C panic=abort \
//         --crate-type=cdylib raydrone.rs -o raydrone.wasm

#![no_std]
#![allow(static_mut_refs)]

// Primitivas DSP compartidas con el motor del VST (un único sitio donde viven).
// El crate es no_std y sin deps, así que el build de `rustc` crudo (sin Cargo) lo
// enlaza vía `--extern raydrone_core=...` — ver build.sh. No toca crates.io.
use raydrone_core::{
    clampf, filter::Filter, sample_at as core_sample_at, soft, sqrtf, tri_inv,
    win_at as core_win_at, DcBlocker, Reverb,
};

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

const SAMPLE_CAP: usize = 8_000_000; // ~180 s @44.1k mono; covers the complete default track
const WIN: usize = 2048;
const MAX_RAYS: usize = 8_000;
const TOP_K: usize = 36;
const MAX_MIX: usize = TOP_K * 3;
const CROSSFADE_S: f32 = 0.012;
const TOP_MIX_COMP: f32 = 0.55;
const GRID_BINS: usize = 256;
const MAX_VOICES: usize = MAX_RAYS;
const BLOCK: usize = 256;
const SLOG_CAP: usize = 512;

static mut SAMPLE: [f32; SAMPLE_CAP] = [0.0; SAMPLE_CAP];
static mut WINDOW: [f32; WIN] = [0.0; WIN];
static mut OUTL: [f32; BLOCK] = [0.0; BLOCK];
static mut OUTR: [f32; BLOCK] = [0.0; BLOCK];

static mut SAMPLE_LEN: usize = 0;
static mut SOURCE_SR: f32 = 44100.0;
static mut OUTPUT_SR: f32 = 44100.0;

// Parámetros base
static mut FOCUS: f32 = 0.3;
static mut APERTURE: f32 = 0.1;
static mut GRAIN_DUR: f32 = 0.15;
static mut GRAIN_RATE: f32 = 200.0;
static mut GAIN: f32 = 0.3;
static mut MASTER: f32 = 1.0;

static mut SPAWN_ACC: f32 = 0.0;
static mut RNG: u32 = 0x1234_5678;

// Muestreo: 0 random, 1 QMC, 2 stratified
static mut MODE: u32 = 1;
static mut QMC: f32 = 0.5;
static mut STRAT_I: u32 = 0;
const GOLDEN: f32 = 0.618_034;
const STRATA: u32 = 17;

// FX
static mut ABER: f32 = 0.0;
static mut A_LOW: f32 = 0.06;
static mut A_HIGH: f32 = 0.35;
static mut BOUNCES: u32 = 0;
static mut REFL: f32 = 0.5;
static mut FEEDBACK: f32 = 0.0;
static mut ENV: f32 = 0.0;
static mut EVO: f32 = 0.5;
static mut EVO_DIR: f32 = 1.0;

// Espacio: ancho estéreo y probabilidad de octava (shimmer)
static mut WIDTH: f32 = 0.0;
static mut OCT: f32 = 0.0;
static mut PITCH_STEP: f32 = 1.0; // multiplicador de velocidad de lectura (transposición)

// Original / Ray direct: una única trayectoria que recorre la fuente sin
// granularizar. No es un bypass de Web Audio: desemboca en exactamente el
// mismo material, filtro y espacio de rayos que el modo Drone.
static mut DIRECT_ON: u32 = 0;
static mut DIRECT_POS: f32 = 0.0;
static mut DIRECT_TONE: f32 = 0.0;
static mut DIRECT_PHASE: f32 = 0.0;

// ── Escala microtonal: tabla de ratios de un período (octava, tritava…).
// Vacía (len=0) = pitch continuo, comportamiento de siempre. Cada grano coge
// un grado de la tabla; el muestreo del grado respeta MODE: estratificado
// recorre los grados como estratos exactos (cobertura homogénea), QMC usa una
// secuencia de Kronecker con la constante plástica (R2), decorrelada de la
// áurea que ya muestrea el eje temporal.
const SCALE_CAP: usize = 64;
static mut SCALE: [f32; SCALE_CAP] = [1.0; SCALE_CAP];
static mut SCALE_LEN: usize = 0;
static mut SCALE_I: u32 = 0; // estratificado: round-robin por grados
static mut QMC_P: f32 = 0.5; // QMC pitch (componente R2)
const R2_ALPHA: f32 = 0.754_877_7; // 1/φ₂, φ₂ = constante plástica ≈ 1.324718

// ── Teclas pulsadas (piano por teclado/ratón/táctil) ────────────────────────
// Ratios de las notas actualmente sostenidas (2^(semitono/12) respecto a la
// raíz), enviados por la UI cada vez que cambia el conjunto de teclas. No
// vacío = cada grano nuevo coge una nota sostenida (así se tocan acordes);
// vacío = se cae al comportamiento de siempre (Microtonal/Voicing o pitch
// continuo). Mismo mecanismo de muestreo por grado que la escala microtonal,
// con su propio acumulador QMC para no correlacionar ambos ejes.
const KEYS_CAP: usize = 16;
static mut KEYS: [f32; KEYS_CAP] = [1.0; KEYS_CAP];
static mut KEYS_LEN: usize = 0;
static mut KEYS_I: u32 = 0; // estratificado: round-robin por nota sostenida
static mut QMC_KEY: f32 = 0.5; // QMC teclas (componente R2, propio)

// ── Ambient: focos múltiples autónomos + árbol recursivo de focos ───────────
// Modo ambient (AMB_ON=1): en vez de un único FOCUS, una constelación de focos.
// Las SEMILLAS se colocan en baja discrepancia (golden ratio) a lo largo del
// sample y derivan solas (paseo aleatorio lento, independiente por foco).
// Recursión: cada cierto tiempo un foco "padre" (depth>0) engendra un foco hijo
// cerca suyo, con offset/apertura/vida que encogen por nivel → estructura
// auto-similar (secciones → frases → granos comparten la misma ley generativa).
// Cada grano elige su foco ponderado por el peso (envolvente de fade del foco).
const FMAX: usize = 48;
static mut FPOS: [f32; FMAX] = [0.0; FMAX];   // posición (segundos)
static mut FVEL: [f32; FMAX] = [0.0; FMAX];   // deriva (seg/seg)
static mut FW: [f32; FMAX] = [0.0; FMAX];     // peso actual (0..1, con fade)
static mut FWT: [f32; FMAX] = [0.0; FMAX];    // peso objetivo
static mut FDEPTH: [u8; FMAX] = [0; FMAX];    // niveles de recursión restantes
static mut FAGE: [f32; FMAX] = [0.0; FMAX];   // edad (s)
static mut FTTL: [f32; FMAX] = [0.0; FMAX];   // vida (s); <0 = inmortal (semilla)
static mut FACT: [bool; FMAX] = [false; FMAX];
static mut AMB_ON: u32 = 0;
static mut AMB_SEEDS: u32 = 3;
static mut AMB_DEPTH: u32 = 2;
static mut AMB_SPREAD: f32 = 0.25; // offset del hijo = SPREAD·span·shrink
static mut AMB_DRIFT: f32 = 0.3;   // velocidad de deriva
static mut AMB_RATE: f32 = 0.4;    // nacimientos de focos por segundo
static mut AMB_DIRTY: bool = true; // recolocar semillas
static mut FOCI_ACC: f32 = 0.0;    // acumulador de nacimientos

// Reverb (Freeverb-lite) y DC blocker viven en el kernel compartido `raydrone_core`
// (el mismo código que enlaza el VST). Una sola instancia estática de cada uno.
static mut REVERB: Reverb = Reverb::new();
static mut DC: DcBlocker = DcBlocker::new();
// Filtro resonante (SVF) con LFO sincronizable a BPM — vive en core::filter.
static mut FILTER: Filter = Filter::new();

// ── Materiales sonoros y modulación ───────────────────────────────────────
// Un material no es un preset de FX: altera el rayo en el momento de nacer y
// durante su lectura. 0 vacío, 1 metal, 2 madera, 3 cristal, 4 agua, 5 plasma.
static mut MATERIAL: u32 = 0;
static mut MATERIAL_AMOUNT: f32 = 1.0;
// Modulador común: 0 off, 1 LFO triangular, 2 envolvente de la salida.
// Destinos: densidad, apertura, pitch. Mantenerlos en el núcleo hace que una
// escena o un SDK obtengan exactamente el mismo resultado que la UI.
static mut MOD_MODE: u32 = 0;
static mut MOD_TARGET: u32 = 0;
static mut MOD_RATE: f32 = 0.25;
static mut MOD_DEPTH: f32 = 0.0;
static mut MOD_ATTACK: f32 = 0.05;
static mut MOD_RELEASE: f32 = 0.5;
static mut MOD_PHASE: f32 = 0.0;
static mut MOD_ENV: f32 = 0.0;

// Cadena espacial ligera después del filtro: delay estéreo + chorus. El
// buffer se comparte entre ambos taps y no asigna memoria en audio real-time.
const DELAY_CAP: usize = 96_000; // 2 s a 48 kHz (se recorta según OUTPUT_SR)
static mut DELAY_L: [f32; DELAY_CAP] = [0.0; DELAY_CAP];
static mut DELAY_R: [f32; DELAY_CAP] = [0.0; DELAY_CAP];
static mut DELAY_W: usize = 0;
static mut DELAY_WET: f32 = 0.0;
static mut DELAY_TIME: f32 = 0.38;
static mut DELAY_FB: f32 = 0.35;
static mut CHORUS_WET: f32 = 0.0;
static mut CHORUS_RATE: f32 = 0.25;
static mut CHORUS_DEPTH: f32 = 0.008;
static mut CHORUS_PHASE: f32 = 0.0;
// Interferencias y objetos vibrantes: FX derivados de trayectorias, no nodos
// Web Audio externos. Flanger = trayectoria casi coincidente; Phaser = fases
// que se cruzan; resonador = objeto que guarda y reemite energía.
static mut FLANGER_WET: f32 = 0.0;
static mut FLANGER_RATE: f32 = 0.16;
static mut FLANGER_DEPTH: f32 = 0.003;
static mut FLANGER_PHASE: f32 = 0.0;
static mut PHASER_WET: f32 = 0.0;
static mut PHASER_RATE: f32 = 0.18;
static mut PHASER_DEPTH: f32 = 0.5;
static mut PHASER_PHASE: f32 = 0.0;
static mut PH_L: [f32; 4] = [0.0; 4];
static mut PH_R: [f32; 4] = [0.0; 4];
static mut DRIVE: f32 = 0.0;
const RES_CAP: usize = 8192;
static mut RES_L: [f32; RES_CAP] = [0.0; RES_CAP];
static mut RES_R: [f32; RES_CAP] = [0.0; RES_CAP];
static mut RES_W: usize = 0;
static mut RES_WET: f32 = 0.0;
static mut RES_FREQ: f32 = 440.0;
static mut RES_DECAY: f32 = 0.55;
// Campo de reflexiones: cuatro trayectorias cortas, con energía que vuelve al
// buffer. Sustituye el uso audible del Freeverb heredado: Space es ahora una
// geometría de rayos, no una caja de reverb convencional.
static mut RAY_REVERB_WET: f32 = 0.0;
static mut RAY_TONE_L: f32 = 0.0;
static mut RAY_TONE_R: f32 = 0.0;
const RAY_CAP: usize = 16_384; // 341 ms @48k; suficiente para cámara y cola recursiva
static mut RAY_L: [f32; RAY_CAP] = [0.0; RAY_CAP];
static mut RAY_R: [f32; RAY_CAP] = [0.0; RAY_CAP];
static mut RAY_W: usize = 0;

// Micro-detune por grano (±0.25% ≈ ±4 cents): batidos entre granos → drone lush, no estático.
const DETUNE: f32 = 0.005;

// ── Trazado inverso: envolvente de energía del sample para lanzar los rayos
// hacia donde hay señal (importance desde la estructura de la fuente).
const EBINS: usize = 1024;
static mut ENERGY: [f32; EBINS] = [0.0; EBINS];
static mut EMAX: f32 = 0.000001;
static mut SMART: u32 = 0; // 1 = rayos inteligentes (trazado inverso)

// Registro de rayos para la visualización (offset en seg + banda + ratio del grado)
static mut SLOG: [f32; SLOG_CAP] = [0.0; SLOG_CAP];
static mut SLOG_B: [f32; SLOG_CAP] = [0.0; SLOG_CAP];
static mut SLOG_S: [f32; SLOG_CAP] = [1.0; SLOG_CAP];
static mut SLOG_W: u32 = 0;

// Diagnóstico de rendimiento
static mut SPAWN_COUNT: u32 = 0; // total de granos disparados (para granos/seg)
// Configuracion musical base. 8K queda disponible como comparacion/estres.
static mut RAY_POPULATION: usize = 2_000;
static mut TOP_COUNT: usize = 0;
static mut TOP_INDEX: [u16; TOP_K] = [0; TOP_K];
static mut TOP_ID: [u32; TOP_K] = [0; TOP_K];
static mut TOP_SCORE: [f32; TOP_K] = [0.0; TOP_K];
static mut TOP_MASK: [bool; MAX_RAYS] = [false; MAX_RAYS];
static mut GRID_COUNT: [u16; GRID_BINS] = [0; GRID_BINS];
static mut TOP_CHANGES: u32 = 0;
static mut TOP_RETENTION: f32 = 1.0;
static mut TOP_RETENTION_SUM: f64 = 0.0;
static mut TOP_TRANSITIONS: u32 = 0;
static mut TOP_ENTRIES: u32 = 0;
static mut TOP_EXITS: u32 = 0;
static mut NEXT_RAY_ID: u32 = 1;
static mut RAY_UPDATE_HZ: f32 = 100.0;
static mut RAY_UPDATE_ACC: f32 = 0.0;
static mut TOP_INERTIA: f32 = 0.25;
static mut MIX_INDEX: [u16; MAX_MIX] = [0; MAX_MIX];
static mut MIX_COUNT: usize = 0;
static mut TOP_METRIC_TICKS: u32 = 0;
static mut TOP_PITCH_MEAN_SUM: f64 = 0.0;
static mut TOP_PITCH_MEDIAN_SUM: f64 = 0.0;
static mut TOP_AGE_MEAN_SUM: f64 = 0.0;
static mut TOP_ENERGY_MEAN_SUM: f64 = 0.0;
static mut TOP_DURATION_MEAN_SUM: f64 = 0.0;

// ── Convergence Lab: el mismo ESTIMADOR del motor (mismo kernel triangular,
// mismo objetivo g[n]), medible offline. Ojo con el matiz para el paper: los
// MUESTREADORES del Lab son las formas canónicas — estratificado de N estratos,
// Kronecker áureo en f64 con rotación Cranley–Patterson, e importance
// reponderado p/q (insesgado) — mientras que el motor en vivo usa variantes
// streaming: recurrencia áurea iterativa en f32 sin rotación, 17 estratos
// fijos round-robin, y los "rayos inteligentes" son exactamente el método
// reverse (rejection sin reponderar, sesgado). Las curvas medidas aquí
// caracterizan las formas canónicas, no las variantes en vivo.
// Corre en una instancia aparte de este módulo dentro de un Web Worker (no toca
// el hilo de audio). Acumuladores en f64 para que el suelo de error medido sea
// del estimador, no de la precisión de la suma. RNG sembrable → reproducible.
const LAB_D: usize = 4096;
const LAB_AMAX: usize = 6600;
const LAB_LEN: usize = 2 * LAB_AMAX + 1;
const LAB_GOLDEN64: f64 = 0.618_033_988_749_894_9;
static mut LAB_WIN: [f32; LAB_D] = [0.0; LAB_D];
static mut LAB_TGT: [f64; LAB_D] = [0.0; LAB_D];
static mut LAB_EST: [f64; LAB_D] = [0.0; LAB_D];
static mut LAB_T32: [f32; LAB_D] = [0.0; LAB_D];
static mut LAB_PM: [f32; LAB_LEN] = [0.0; LAB_LEN];
static mut LAB_QM: [f32; LAB_LEN] = [0.0; LAB_LEN];
static mut LAB_CUM: [f32; LAB_LEN] = [0.0; LAB_LEN];
static mut LAB_EN: [f32; LAB_LEN] = [0.0; LAB_LEN];
static mut LAB_EMAX: f32 = 0.000_001;

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    id: u32,
    mix_gain: f32,
    mix_target: f32,
    pos: f32,
    age: f32,
    inv_dur: f32,
    gain: f32,
    band: u8,
    lp: f32,
    tone: f32,
    depth: u32,
    step: f32, // velocidad de lectura (1.0 normal, 2.0 octava arriba)
    panl: f32,
    panr: f32,
}

static mut VOICES: [Voice; MAX_VOICES] = [Voice {
    active: false,
    id: 0,
    mix_gain: 0.0,
    mix_target: 0.0,
    pos: 0.0,
    age: 0.0,
    inv_dur: 0.0,
    gain: 0.0,
    band: 1,
    lp: 0.0,
    tone: 0.0,
    depth: 0,
    step: 1.0,
    panl: 0.707,
    panr: 0.707,
}; MAX_VOICES];

// Lista compacta de voces activas + pila de slots libres: el bucle caliente
// recorre solo las voces que suenan (O(activas)), no las 512.
static mut ACTIVE: [u16; MAX_VOICES] = [0; MAX_VOICES];
static mut NACTIVE: usize = 0;
static mut FREE: [u16; MAX_VOICES] = [0; MAX_VOICES];
static mut NFREE: usize = 0;
static mut VINIT: bool = false;

#[inline]
fn ensure_voice_init() {
    unsafe {
        if !VINIT {
            for i in 0..MAX_VOICES {
                FREE[i] = (MAX_VOICES - 1 - i) as u16;
            }
            NFREE = MAX_VOICES;
            NACTIVE = 0;
            VINIT = true;
        }
    }
}

fn reset_runtime_state() {
    unsafe {
        for i in 0..MAX_VOICES {
            VOICES[i].active = false;
            FREE[i] = (MAX_VOICES - 1 - i) as u16;
        }
        NFREE = MAX_VOICES;
        NACTIVE = 0;
        VINIT = true;

        SPAWN_ACC = 0.0;
        QMC = 0.5;
        STRAT_I = 0;
        SCALE_I = 0;
        QMC_P = 0.5;
        KEYS_I = 0;
        QMC_KEY = 0.5;
        ENV = 0.0;
        EVO = 0.5;
        EVO_DIR = 1.0;
        FOCI_ACC = 0.0;
        AMB_DIRTY = true;
        for i in 0..FMAX {
            FPOS[i] = 0.0;
            FVEL[i] = 0.0;
            FW[i] = 0.0;
            FWT[i] = 0.0;
            FDEPTH[i] = 0;
            FAGE[i] = 0.0;
            FTTL[i] = 0.0;
            FACT[i] = false;
        }
        SLOG_W = 0;
        SPAWN_COUNT = 0;
        TOP_COUNT = 0;
        TOP_CHANGES = 0;
        TOP_RETENTION = 1.0;
        TOP_RETENTION_SUM = 0.0;
        TOP_TRANSITIONS = 0;
        TOP_ENTRIES = 0;
        TOP_EXITS = 0;
        NEXT_RAY_ID = 1;
        RAY_UPDATE_ACC = 0.0;
        MIX_COUNT = 0;
        TOP_METRIC_TICKS = 0;
        TOP_PITCH_MEAN_SUM = 0.0;
        TOP_PITCH_MEDIAN_SUM = 0.0;
        TOP_AGE_MEAN_SUM = 0.0;
        TOP_ENERGY_MEAN_SUM = 0.0;
        TOP_DURATION_MEAN_SUM = 0.0;
        for i in 0..MAX_RAYS { TOP_MASK[i] = false; }

        REVERB.reset();
        DC.reset();
        FILTER.reset();
    }
}

#[inline]
fn rng01() -> f32 {
    unsafe { raydrone_core::rng01(&mut RNG) }
}

#[inline]
fn next_u() -> f32 {
    unsafe {
        match MODE {
            1 => {
                QMC += GOLDEN;
                if QMC >= 1.0 {
                    QMC -= 1.0;
                }
                QMC
            }
            2 => {
                let u = (STRAT_I as f32 + rng01()) / (STRATA as f32);
                STRAT_I = (STRAT_I + 1) % STRATA;
                u
            }
            _ => rng01(),
        }
    }
}

fn update_coeffs() {
    unsafe {
        let tp = 6.283_185_5f32;
        A_LOW = clampf(tp * 500.0 / OUTPUT_SR, 0.0, 0.99);
        A_HIGH = clampf(tp * 2500.0 / OUTPUT_SR, 0.0, 0.99);
        // DC blocker (~10 Hz) y longitudes de reverb escaladas al SR real.
        DC.set_sample_rate(OUTPUT_SR);
        REVERB.set_sample_rate(OUTPUT_SR);
        FILTER.set_sample_rate(OUTPUT_SR);
    }
}

// ── Punteros / capacidad ───────────────────────────────────────────────────
#[no_mangle]
pub extern "C" fn sample_ptr() -> *mut f32 {
    unsafe { SAMPLE.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn window_ptr() -> *mut f32 {
    unsafe { WINDOW.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn window_capacity() -> usize {
    WIN
}
#[no_mangle]
pub extern "C" fn block_capacity() -> usize {
    BLOCK
}
#[no_mangle]
pub extern "C" fn out_l_ptr() -> *mut f32 {
    unsafe { OUTL.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn out_r_ptr() -> *mut f32 {
    unsafe { OUTR.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn sample_capacity() -> usize {
    SAMPLE_CAP
}
#[no_mangle]
pub extern "C" fn slog_ptr() -> *mut f32 {
    unsafe { SLOG.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn slog_b_ptr() -> *mut f32 {
    unsafe { SLOG_B.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn slog_s_ptr() -> *mut f32 {
    unsafe { SLOG_S.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn slog_w() -> u32 {
    unsafe { SLOG_W }
}
#[no_mangle]
pub extern "C" fn slog_cap() -> usize {
    SLOG_CAP
}
#[no_mangle]
pub extern "C" fn out_level() -> f32 {
    unsafe { ENV }
}
// Diagnóstico: nº de voces (rayos) activas mezcladas por bloque.
#[no_mangle]
pub extern "C" fn active_voices() -> u32 {
    unsafe { TOP_COUNT as u32 }
}
#[no_mangle]
pub extern "C" fn simulated_rays() -> u32 { unsafe { NACTIVE as u32 } }
#[no_mangle]
pub extern "C" fn ray_population() -> u32 { unsafe { RAY_POPULATION as u32 } }
#[no_mangle]
pub extern "C" fn top36_changes() -> u32 { unsafe { TOP_CHANGES } }
#[no_mangle]
pub extern "C" fn top36_retention() -> f32 { unsafe { TOP_RETENTION } }
#[no_mangle]
pub extern "C" fn top36_mean_retention() -> f32 {
    unsafe { if TOP_TRANSITIONS == 0 { 1.0 } else { (TOP_RETENTION_SUM / TOP_TRANSITIONS as f64) as f32 } }
}
#[no_mangle]
pub extern "C" fn top36_entries() -> u32 { unsafe { TOP_ENTRIES } }
#[no_mangle]
pub extern "C" fn top36_exits() -> u32 { unsafe { TOP_EXITS } }
#[no_mangle]
pub extern "C" fn top36_transitions() -> u32 { unsafe { TOP_TRANSITIONS } }
#[no_mangle]
pub extern "C" fn set_ray_population(n: u32) {
    unsafe { RAY_POPULATION = (n as usize).max(TOP_K).min(MAX_RAYS); }
}
#[no_mangle]
pub extern "C" fn set_ray_update_hz(hz: f32) {
    unsafe { RAY_UPDATE_HZ = if hz.is_finite() { clampf(hz, 1.0, 1000.0) } else { 100.0 }; }
}
#[no_mangle]
pub extern "C" fn set_top36_inertia(amount: f32) {
    unsafe {
        TOP_INERTIA = if amount.is_finite() { clampf(amount, 0.0, 1.0) } else { 0.0 };
        // Una nueva posición del control inicia una ventana estadística limpia.
        TOP_RETENTION = 1.0;
        TOP_RETENTION_SUM = 0.0;
        TOP_TRANSITIONS = 0;
        TOP_ENTRIES = 0;
        TOP_EXITS = 0;
    }
}
#[no_mangle]
pub extern "C" fn top36_inertia() -> f32 { unsafe { TOP_INERTIA } }
#[inline]
fn metric_average(sum: f64) -> f32 {
    unsafe { if TOP_METRIC_TICKS == 0 { 0.0 } else { (sum / TOP_METRIC_TICKS as f64) as f32 } }
}
#[no_mangle]
pub extern "C" fn top36_pitch_mean() -> f32 { unsafe { metric_average(TOP_PITCH_MEAN_SUM) } }
#[no_mangle]
pub extern "C" fn top36_pitch_median() -> f32 { unsafe { metric_average(TOP_PITCH_MEDIAN_SUM) } }
#[no_mangle]
pub extern "C" fn top36_age_mean_s() -> f32 { unsafe { metric_average(TOP_AGE_MEAN_SUM) } }
#[no_mangle]
pub extern "C" fn top36_energy_mean() -> f32 { unsafe { metric_average(TOP_ENERGY_MEAN_SUM) } }
#[no_mangle]
pub extern "C" fn top36_effective_duration_mean_s() -> f32 { unsafe { metric_average(TOP_DURATION_MEAN_SUM) } }
#[no_mangle]
pub extern "C" fn spawn_count() -> u32 {
    unsafe { SPAWN_COUNT }
}

// ── Setters ────────────────────────────────────────────────────────────────
#[no_mangle]
pub extern "C" fn set_sample(len: usize, sr: f32) {
    unsafe {
        SAMPLE_LEN = if len > SAMPLE_CAP { SAMPLE_CAP } else { len };
        SOURCE_SR = if sr.is_finite() && sr > 1.0 { sr } else { 44100.0 };
    }
    reset_runtime_state();
    build_energy();
}

#[no_mangle]
pub extern "C" fn set_output_sample_rate(sr: f32) {
    unsafe {
        OUTPUT_SR = if sr.is_finite() && sr > 1.0 { sr } else { 44100.0 };
    }
    update_coeffs();
}

// Envolvente de energía (RMS por bin) de todo el sample, para el trazado inverso.
fn build_energy() {
    unsafe {
        EMAX = 0.000001;
        if SAMPLE_LEN < 2 {
            let mut b = 0;
            while b < EBINS {
                ENERGY[b] = 0.0;
                b += 1;
            }
            return;
        }
        for b in 0..EBINS {
            let start = b * SAMPLE_LEN / EBINS;
            let end = ((b + 1) * SAMPLE_LEN / EBINS).min(SAMPLE_LEN);
            let mut s = 0.0f32;
            let mut cnt = 0u32;
            let mut i = start;
            while i < end {
                let v = SAMPLE[i];
                s += v * v;
                cnt += 1;
                i += 4; // stride para abaratar
            }
            let e = if cnt > 0 { sqrtf(s / (cnt as f32)) } else { 0.0 };
            ENERGY[b] = e;
            if e > EMAX {
                EMAX = e;
            }
        }
    }
}

#[inline]
fn energy_at(sec: f32) -> f32 {
    unsafe {
        let span = (SAMPLE_LEN as f32) / SOURCE_SR;
        if span <= 0.0 {
            return 0.0;
        }
        let mut b = (sec / span * (EBINS as f32)) as usize;
        if b >= EBINS {
            b = EBINS - 1;
        }
        ENERGY[b]
    }
}

#[no_mangle]
pub extern "C" fn set_smart(on: u32) {
    unsafe {
        SMART = on;
    }
}

#[no_mangle]
pub extern "C" fn set_reverb(wet: f32) {
    unsafe {
        RAY_REVERB_WET = if wet.is_finite() { clampf(wet, 0.0, 0.82) } else { 0.0 };
        // Compatibilidad ABI: mantenemos el objeto compartido, pero el campo
        // de trayectorias de abajo es la única reverb audible en RayDrone.
        REVERB.set_wet(0.0);
    }
}

// Filtro resonante: cutoff en Hz (>= ~Nyquist = abierto/bypass) y resonancia 0..1.
#[no_mangle]
pub extern "C" fn set_filter(cutoff_hz: f32, res: f32) {
    unsafe {
        let cutoff = if cutoff_hz.is_finite() { clampf(cutoff_hz, 10.0, OUTPUT_SR * 0.5) } else { OUTPUT_SR * 0.5 };
        let resonance = if res.is_finite() { clampf(res, 0.0, 1.0) } else { 0.0 };
        FILTER.set(cutoff, resonance);
    }
}

// LFO de cutoff: rate en Hz (la UI lo deriva de BPM + división de nota) y
// profundidad en octavas (0 = LFO apagado). Sincroniza el barrido al tempo.
#[no_mangle]
pub extern "C" fn set_filter_lfo(rate_hz: f32, depth_oct: f32) {
    unsafe {
        let rate = if rate_hz.is_finite() { clampf(rate_hz, 0.0, 100.0) } else { 0.0 };
        let depth = if depth_oct.is_finite() { clampf(depth_oct, 0.0, 12.0) } else { 0.0 };
        FILTER.set_lfo(rate, depth);
    }
}

// Materiales: API estable para UI, escenas y SDK. El valor se valida aquí para
// que un mensaje de terceros nunca pueda llevar el render a NaN.
#[no_mangle]
pub extern "C" fn set_material(kind: u32, amount: f32) {
    unsafe {
        MATERIAL = kind.min(5);
        MATERIAL_AMOUNT = if amount.is_finite() { clampf(amount, 0.0, 1.0) } else { 0.0 };
    }
}

#[no_mangle]
pub extern "C" fn set_modulation(mode: u32, target: u32, rate_hz: f32, depth: f32, attack_s: f32, release_s: f32) {
    unsafe {
        MOD_MODE = mode.min(2);
        MOD_TARGET = target.min(2);
        MOD_RATE = if rate_hz.is_finite() { clampf(rate_hz, 0.001, 30.0) } else { 0.25 };
        MOD_DEPTH = if depth.is_finite() { clampf(depth, 0.0, 1.0) } else { 0.0 };
        MOD_ATTACK = if attack_s.is_finite() { clampf(attack_s, 0.001, 10.0) } else { 0.05 };
        MOD_RELEASE = if release_s.is_finite() { clampf(release_s, 0.001, 10.0) } else { 0.5 };
    }
}

#[no_mangle]
pub extern "C" fn set_effects(delay_wet: f32, delay_time_s: f32, delay_feedback: f32, chorus_wet: f32, chorus_rate: f32, chorus_depth_s: f32) {
    unsafe {
        DELAY_WET = if delay_wet.is_finite() { clampf(delay_wet, 0.0, 1.0) } else { 0.0 };
        DELAY_TIME = if delay_time_s.is_finite() { clampf(delay_time_s, 0.01, 1.8) } else { 0.38 };
        // El feedback se limita por debajo de la auto-oscilación audible.
        DELAY_FB = if delay_feedback.is_finite() { clampf(delay_feedback, 0.0, 0.68) } else { 0.35 };
        CHORUS_WET = if chorus_wet.is_finite() { clampf(chorus_wet, 0.0, 1.0) } else { 0.0 };
        CHORUS_RATE = if chorus_rate.is_finite() { clampf(chorus_rate, 0.01, 8.0) } else { 0.25 };
        CHORUS_DEPTH = if chorus_depth_s.is_finite() { clampf(chorus_depth_s, 0.001, 0.03) } else { 0.008 };
    }
}

#[no_mangle]
pub extern "C" fn set_advanced_effects(flanger_wet: f32, flanger_rate: f32, flanger_depth_s: f32, phaser_wet: f32, phaser_rate: f32, phaser_depth: f32, drive: f32, resonator_wet: f32, resonator_hz: f32, resonator_decay: f32) {
    unsafe {
        FLANGER_WET = if flanger_wet.is_finite() { clampf(flanger_wet, 0.0, 1.0) } else { 0.0 };
        FLANGER_RATE = if flanger_rate.is_finite() { clampf(flanger_rate, 0.01, 8.0) } else { 0.16 };
        FLANGER_DEPTH = if flanger_depth_s.is_finite() { clampf(flanger_depth_s, 0.0001, 0.015) } else { 0.003 };
        PHASER_WET = if phaser_wet.is_finite() { clampf(phaser_wet, 0.0, 1.0) } else { 0.0 };
        PHASER_RATE = if phaser_rate.is_finite() { clampf(phaser_rate, 0.01, 8.0) } else { 0.18 };
        PHASER_DEPTH = if phaser_depth.is_finite() { clampf(phaser_depth, 0.0, 1.0) } else { 0.5 };
        DRIVE = if drive.is_finite() { clampf(drive, 0.0, 1.0) } else { 0.0 };
        RES_WET = if resonator_wet.is_finite() { clampf(resonator_wet, 0.0, 1.0) } else { 0.0 };
        RES_FREQ = if resonator_hz.is_finite() { clampf(resonator_hz, 60.0, 4000.0) } else { 440.0 };
        RES_DECAY = if resonator_decay.is_finite() { clampf(resonator_decay, 0.0, 0.92) } else { 0.55 };
    }
}

#[no_mangle]
pub extern "C" fn set_params(focus: f32, aperture: f32, grain_ms: f32, grain_rate: f32, gain: f32, master: f32) {
    unsafe {
        FOCUS = if focus.is_finite() { focus } else { 0.0 };
        APERTURE = if aperture.is_finite() { clampf(aperture, 0.0, 3600.0) } else { 0.0 };
        GRAIN_DUR = if grain_ms.is_finite() { clampf(grain_ms, 1.0, 10_000.0) * 0.001 } else { 0.15 };
        GRAIN_RATE = if grain_rate.is_finite() { clampf(grain_rate, 0.0, 100_000.0) } else { 0.0 };
        GAIN = if gain.is_finite() { clampf(gain, 0.0, 8.0) } else { 0.0 };
        MASTER = if master.is_finite() { clampf(master, 0.0, 8.0) } else { 0.0 };
    }
}

#[no_mangle]
pub extern "C" fn set_mode(m: u32) {
    unsafe {
        MODE = m.min(2);
    }
}

#[no_mangle]
pub extern "C" fn set_fx(aber: f32, bounces: u32, refl: f32, feedback: f32) {
    unsafe {
        ABER = if aber.is_finite() { clampf(aber, 0.0, 1.0) } else { 0.0 };
        BOUNCES = bounces.min(6); // same cap as the VST engine's set_bounce
        REFL = if refl.is_finite() { clampf(refl, 0.0, 1.0) } else { 0.0 };
        FEEDBACK = if feedback.is_finite() { clampf(feedback, 0.0, 1.0) } else { 0.0 };
        update_coeffs();
    }
}

#[no_mangle]
pub extern "C" fn set_space(width: f32, oct: f32) {
    unsafe {
        WIDTH = if width.is_finite() { clampf(width, 0.0, 1.0) } else { 0.0 };
        OCT = if oct.is_finite() { clampf(oct, 0.0, 1.0) } else { 0.0 };
    }
}

// Transposición: el multiplicador 2^(semitons/12) se calcula en JS (sin exp en no_std).
#[no_mangle]
pub extern "C" fn set_pitch(mult: f32) {
    unsafe {
        PITCH_STEP = if mult.is_finite() { clampf(mult, 0.01, 32.0) } else { 1.0 };
    }
}

// ── Escala microtonal ──
#[no_mangle]
pub extern "C" fn scale_ptr() -> *mut f32 {
    unsafe { SCALE.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn scale_capacity() -> usize {
    SCALE_CAP
}
#[no_mangle]
pub extern "C" fn set_scale(len: usize) {
    unsafe {
        SCALE_LEN = if len > SCALE_CAP { SCALE_CAP } else { len };
        for ratio in &mut SCALE[..SCALE_LEN] {
            if !ratio.is_finite() || *ratio <= 0.0 {
                *ratio = 1.0;
            }
        }
        SCALE_I = 0;
    }
}

// Cargar una de las voces armónicas curadas de `core::music` (acordes/escalas en
// entonación justa) en la tabla de escala — cada grano coge un grado, así la nube
// se apila en un pad afinado en vez de un lavado a una sola altura. preset 0 =
// unísono (continuo de siempre).
#[no_mangle]
pub extern "C" fn set_chord(preset: u32) {
    unsafe {
        if preset == raydrone_core::music::CHORD_UNISON {
            SCALE_LEN = 0;
            SCALE_I = 0;
            return;
        }
        let ratios = raydrone_core::music::chord_ratios(preset);
        let n = if ratios.len() > SCALE_CAP { SCALE_CAP } else { ratios.len() };
        for i in 0..n {
            SCALE[i] = ratios[i];
        }
        SCALE_LEN = n;
        SCALE_I = 0;
    }
}

// Grado de la escala para el grano nuevo. La dimensión pitch se muestrea
// aparte de la temporal: estratificado puro sobre los grados (cada grado es
// un estrato → ningún micro-intervalo queda sin cubrir) o Kronecker R2 en QMC.
#[inline]
fn scale_ratio() -> f32 {
    unsafe {
        if SCALE_LEN == 0 {
            return 1.0;
        }
        let n = SCALE_LEN;
        let idx = match MODE {
            1 => {
                QMC_P += R2_ALPHA;
                if QMC_P >= 1.0 {
                    QMC_P -= 1.0;
                }
                (QMC_P * (n as f32)) as usize
            }
            2 => {
                let i = (SCALE_I as usize) % n;
                SCALE_I = SCALE_I.wrapping_add(1);
                i
            }
            _ => (rng01() * (n as f32)) as usize,
        };
        SCALE[if idx >= n { n - 1 } else { idx }]
    }
}

// ── Teclas pulsadas (piano) ──
#[no_mangle]
pub extern "C" fn keys_ptr() -> *mut f32 {
    unsafe { KEYS.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn keys_capacity() -> usize {
    KEYS_CAP
}
// La UI llama a esto cada vez que cambia el conjunto de teclas sostenidas
// (teclado, ratón o táctil), con los ratios ya escritos en `keys_ptr()`.
// len=0 = ninguna tecla pulsada → se cae a Microtonal/Voicing o pitch continuo.
#[no_mangle]
pub extern "C" fn set_keys(len: usize) {
    unsafe {
        KEYS_LEN = if len > KEYS_CAP { KEYS_CAP } else { len };
        for ratio in &mut KEYS[..KEYS_LEN] {
            if !ratio.is_finite() || *ratio <= 0.0 {
                *ratio = 1.0;
            }
        }
        KEYS_I = 0;
    }
}

// Nota para el grano nuevo, elegida entre las teclas sostenidas — mismo
// mecanismo de muestreo por grado que `scale_ratio`, con su propio
// acumulador QMC para no correlacionar los dos ejes de "grado".
#[inline]
fn key_ratio() -> f32 {
    unsafe {
        if KEYS_LEN == 0 {
            return 1.0;
        }
        let n = KEYS_LEN;
        let idx = match MODE {
            1 => {
                QMC_KEY += R2_ALPHA;
                if QMC_KEY >= 1.0 {
                    QMC_KEY -= 1.0;
                }
                (QMC_KEY * (n as f32)) as usize
            }
            2 => {
                let i = (KEYS_I as usize) % n;
                KEYS_I = KEYS_I.wrapping_add(1);
                i
            }
            _ => (rng01() * (n as f32)) as usize,
        };
        KEYS[if idx >= n { n - 1 } else { idx }]
    }
}

// ── Ambient: focos múltiples + recursión ────────────────────────────────────
#[no_mangle]
pub extern "C" fn set_ambient(on: u32, seeds: u32, depth: u32, spread: f32, drift: f32, rate: f32) {
    unsafe {
        let was = AMB_ON;
        AMB_ON = if on != 0 { 1 } else { 0 };
        let ns = if seeds < 1 { 1 } else if seeds > 8 { 8 } else { seeds };
        if ns != AMB_SEEDS || was != AMB_ON {
            AMB_DIRTY = true; // recolocar semillas si cambia el nº o se enciende
        }
        AMB_SEEDS = ns;
        AMB_DEPTH = if depth > 4 { 4 } else { depth };
        AMB_SPREAD = if spread.is_finite() { clampf(spread, 0.0, 1.0) } else { 0.0 };
        AMB_DRIFT = if drift.is_finite() { clampf(drift, 0.0, 1.0) } else { 0.0 };
        AMB_RATE = if rate.is_finite() { clampf(rate, 0.0, 4.0) } else { 0.0 };
    }
}

#[inline]
fn powf_i(base: f32, n: u32) -> f32 {
    let mut r = 1.0f32;
    let mut i = 0;
    while i < n {
        r *= base;
        i += 1;
    }
    r
}

#[inline]
fn focus_span() -> f32 {
    unsafe {
        let s = (SAMPLE_LEN as f32) / SOURCE_SR;
        if s > 0.0 {
            s
        } else {
            0.0
        }
    }
}

// (Re)colocar las semillas en baja discrepancia (golden ratio) a lo largo del
// sample: máxima dispersión, sin solapes. Inmortales y a peso pleno.
fn reinit_foci() {
    unsafe {
        let span = focus_span();
        for i in 0..FMAX {
            FACT[i] = false;
            FW[i] = 0.0;
        }
        let ns = AMB_SEEDS as usize;
        for i in 0..ns {
            let g = 0.5 + (i as f32) * 0.618_034;
            let u = g - (g as u32 as f32); // parte fraccionaria (g siempre > 0)
            FPOS[i] = u * span;
            FVEL[i] = (rng01() - 0.5) * AMB_DRIFT * span * 0.02;
            FW[i] = 1.0;
            FWT[i] = 1.0;
            FDEPTH[i] = AMB_DEPTH as u8;
            FAGE[i] = 0.0;
            FTTL[i] = -1.0; // inmortal
            FACT[i] = true;
        }
        FOCI_ACC = 0.0;
        AMB_DIRTY = false;
    }
}

fn free_focus_slot() -> i32 {
    unsafe {
        for i in 0..FMAX {
            if !FACT[i] {
                return i as i32;
            }
        }
        -1
    }
}

// Engendrar un foco hijo: padre elegido ponderado entre los que aún tienen
// niveles (depth>0). El hijo cae cerca, con offset/vida/peso que encogen por
// nivel → árbol auto-similar.
fn birth_focus() {
    unsafe {
        let slot = free_focus_slot();
        if slot < 0 {
            return;
        }
        // padre ponderado por peso, con depth>0
        let mut sum = 0.0f32;
        for i in 0..FMAX {
            if FACT[i] && FDEPTH[i] > 0 {
                sum += FW[i];
            }
        }
        if sum <= 0.0 {
            return;
        }
        let mut r = rng01() * sum;
        let mut parent = -1i32;
        for i in 0..FMAX {
            if FACT[i] && FDEPTH[i] > 0 {
                r -= FW[i];
                if r <= 0.0 {
                    parent = i as i32;
                    break;
                }
            }
        }
        if parent < 0 {
            return;
        }
        let p = parent as usize;
        let level = (AMB_DEPTH as i32 - FDEPTH[p] as i32 + 1).max(0) as u32; // 1 en el primer nivel
        let shrink = powf_i(0.55, level);
        let span = focus_span();
        let off = (rng01() - 0.5) * 2.0 * AMB_SPREAD * span * shrink;
        let s = slot as usize;
        FPOS[s] = clampf(FPOS[p] + off, 0.0, span);
        FVEL[s] = (rng01() - 0.5) * AMB_DRIFT * span * 0.02 * (1.0 + shrink);
        FW[s] = 0.0; // fade-in
        FWT[s] = FW[p] * 0.72; // hijos más discretos
        FDEPTH[s] = FDEPTH[p] - 1;
        FAGE[s] = 0.0;
        FTTL[s] = 12.0 * shrink; // vida finita, más corta a escala fina
        FACT[s] = true;
    }
}

// Avanzar la constelación dt segundos: deriva, fades, nacimientos, muertes.
fn update_foci(dt: f32) {
    unsafe {
        if AMB_ON == 0 {
            return;
        }
        let span = focus_span();
        if span <= 0.0 {
            return;
        }
        if AMB_DIRTY {
            reinit_foci();
        }
        let maxv = AMB_DRIFT * span * 0.03;
        let fade = if dt / 1.5 < 1.0 { dt / 1.5 } else { 1.0 };
        for i in 0..FMAX {
            if !FACT[i] {
                continue;
            }
            FAGE[i] += dt;
            // deriva: re-aleatorizar velocidad de vez en cuando (~cada 3 s)
            if rng01() < dt / 3.0 {
                FVEL[i] = (rng01() - 0.5) * 2.0 * maxv;
            }
            let mut pos = FPOS[i] + FVEL[i] * dt;
            if pos < 0.0 {
                pos = -pos;
                FVEL[i] = -FVEL[i];
            } else if pos > span {
                pos = 2.0 * span - pos;
                FVEL[i] = -FVEL[i];
            }
            FPOS[i] = clampf(pos, 0.0, span);
            // envolvente de peso: los mortales se apagan al acercarse a su TTL
            if FTTL[i] >= 0.0 && FAGE[i] > FTTL[i] - 1.5 {
                FWT[i] = 0.0;
            }
            FW[i] += (FWT[i] - FW[i]) * fade;
            if FTTL[i] >= 0.0 && FAGE[i] > FTTL[i] && FW[i] < 0.002 {
                FACT[i] = false;
                FW[i] = 0.0;
            }
        }
        // nacimientos
        if AMB_DEPTH > 0 {
            FOCI_ACC += AMB_RATE * dt;
            let mut guard = 0;
            while FOCI_ACC >= 1.0 && guard < 8 {
                birth_focus();
                FOCI_ACC -= 1.0;
                guard += 1;
            }
        }
    }
}

// Elegir foco ponderado por peso → (índice, factor de apertura por nivel).
// Devuelve índice -1 si no hay constelación (cae al FOCUS único de siempre).
fn pick_focus() -> i32 {
    unsafe {
        let mut sum = 0.0f32;
        for i in 0..FMAX {
            if FACT[i] {
                sum += FW[i];
            }
        }
        if sum <= 0.0 {
            return -1;
        }
        let mut r = rng01() * sum;
        for i in 0..FMAX {
            if FACT[i] {
                r -= FW[i];
                if r <= 0.0 {
                    return i as i32;
                }
            }
        }
        -1
    }
}

// Punteros para la visualización de la constelación (posición en seg + peso).
#[no_mangle]
pub extern "C" fn foci_ptr() -> *mut f32 {
    unsafe { FPOS.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn foci_w_ptr() -> *mut f32 {
    unsafe { FW.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn foci_cap() -> usize {
    FMAX
}

#[no_mangle]
pub extern "C" fn seed(s: u32) {
    unsafe {
        RNG = if s == 0 { 1 } else { s };
    }
}

// ── Convergence Lab ─────────────────────────────────────────────────────────
#[inline]
fn lab_s(i: i64) -> f64 {
    unsafe {
        if i >= 0 && (i as usize) < SAMPLE_LEN {
            SAMPLE[i as usize] as f64
        } else {
            0.0
        }
    }
}

#[inline]
fn fract64(x: f64) -> f64 {
    x - (x as i64 as f64) // válido para x >= 0 (aquí siempre)
}

#[inline]
fn roundi(x: f32) -> i64 {
    if x >= 0.0 {
        (x + 0.5) as i64
    } else {
        -(((-x) + 0.5) as i64)
    }
}

#[inline]
fn lab_clamp_a(a: i32) -> i32 {
    if a < 1 {
        1
    } else if a as usize > LAB_AMAX {
        LAB_AMAX as i32
    } else {
        a
    }
}

#[no_mangle]
pub extern "C" fn lab_win_ptr() -> *mut f32 {
    unsafe { LAB_WIN.as_mut_ptr() }
}
#[no_mangle]
pub extern "C" fn lab_target_ptr() -> *mut f32 {
    unsafe { LAB_T32.as_mut_ptr() } // copia f32 del objetivo (para el A/B audible)
}
#[no_mangle]
pub extern "C" fn lab_grain() -> usize {
    LAB_D
}
#[no_mangle]
pub extern "C" fn lab_max_a() -> usize {
    LAB_AMAX
}

// Objetivo determinista: tgt[n] = win[n] · Σₖ p(k)·s(f0+k+n), p triangular.
#[no_mangle]
pub extern "C" fn lab_target(f0: i32, a: i32) {
    unsafe {
        let a = lab_clamp_a(a);
        let inv_a2 = 1.0f64 / ((a as f64) * (a as f64));
        for n in 0..LAB_D {
            LAB_TGT[n] = 0.0;
        }
        let mut k = -a;
        while k <= a {
            let pk = ((a - if k < 0 { -k } else { k }) as f64) * inv_a2;
            if pk > 0.0 {
                let b = f0 as i64 + k as i64;
                for n in 0..LAB_D {
                    LAB_TGT[n] += pk * lab_s(b + n as i64);
                }
            }
            k += 1;
        }
        for n in 0..LAB_D {
            LAB_TGT[n] *= LAB_WIN[n] as f64;
            LAB_T32[n] = LAB_TGT[n] as f32;
        }
    }
}

// Distribuciones para importance/reverse: p (triangular), q ∝ p·energía, CDF y energía.
#[no_mangle]
pub extern "C" fn lab_imp_build(f0: i32, a: i32) {
    unsafe {
        let a = lab_clamp_a(a);
        let len = (2 * a + 1) as usize;
        let stride = 8usize;
        let taps = (LAB_D + stride - 1) / stride;
        let inv_a2 = 1.0f32 / ((a * a) as f32);
        let mut ps = 0.0f32;
        let mut qs = 0.0f32;
        LAB_EMAX = 0.000_001;
        for idx in 0..len {
            let k = idx as i32 - a;
            let p = ((a - k.abs()) as f32) * inv_a2;
            let b = f0 as i64 + k as i64;
            let mut s = 0.0f32;
            let mut n = 0usize;
            while n < LAB_D {
                let v = lab_s(b + n as i64) as f32;
                s += v * v;
                n += stride;
            }
            let e = sqrtf(s / (taps as f32)) + 0.000_001;
            LAB_EN[idx] = e;
            if e > LAB_EMAX {
                LAB_EMAX = e;
            }
            LAB_PM[idx] = p;
            LAB_QM[idx] = p * e;
            ps += p;
            qs += p * e;
        }
        let mut acc = 0.0f32;
        for idx in 0..len {
            LAB_PM[idx] /= ps;
            LAB_QM[idx] = if qs > 0.0 { LAB_QM[idx] / qs } else { 1.0 / (len as f32) };
            acc += LAB_QM[idx];
            LAB_CUM[idx] = acc;
        }
    }
}

#[inline]
fn lab_cum_search(len: usize, r: f32) -> usize {
    unsafe {
        let mut lo = 0usize;
        let mut hi = len - 1;
        while lo < hi {
            let m = (lo + hi) >> 1;
            if LAB_CUM[m] < r {
                lo = m + 1;
            } else {
                hi = m;
            }
        }
        lo
    }
}

// Una estimación con N rayos. method: 0 random, 1 stratified, 2 QMC (áurea),
// 3 importance (reponderado, insesgado), 4 reverse (q ∝ p·energía, sesgado).
#[no_mangle]
pub extern "C" fn lab_estimate(f0: i32, a: i32, n_rays: u32, method: u32, sd: u32) {
    unsafe {
        let a = lab_clamp_a(a);
        RNG = if sd == 0 { 1 } else { sd };
        let n = if n_rays < 1 { 1 } else { n_rays } as usize;
        for m in 0..LAB_D {
            LAB_EST[m] = 0.0;
        }
        let rot = rng01() as f64;
        if method == 3 {
            let len = (2 * a + 1) as usize;
            for i in 0..n {
                let u = ((i as f32) + rng01()) / (n as f32);
                let idx = lab_cum_search(len, u);
                // Borde f32: con N grande, u puede redondear a 1.0 (o superar el
                // máximo de la CDF, que es una suma acumulada en f32) y la
                // búsqueda devuelve el bin extremo de la apertura, donde el
                // kernel triangular vale exactamente 0 → p = q = 0 y 0/0 = NaN
                // envenenaría la estimación entera. Un bin de masa nula no
                // debería poder salir elegido: su peso correcto es 0, así que
                // se descarta la muestra (equivale a wi = 0, sin sesgo).
                let qm = LAB_QM[idx];
                if qm <= 0.0 {
                    continue;
                }
                let k = idx as i32 - a;
                let wi = (LAB_PM[idx] / qm) as f64;
                let b = f0 as i64 + k as i64;
                for m in 0..LAB_D {
                    LAB_EST[m] += wi * lab_s(b + m as i64);
                }
            }
            let inv = 1.0 / (n as f64);
            for m in 0..LAB_D {
                LAB_EST[m] *= inv * (LAB_WIN[m] as f64);
            }
        } else if method == 4 {
            for _ in 0..n {
                let idx = lab_cum_search((2 * a + 1) as usize, rng01());
                let b = f0 as i64 + idx as i64 - a as i64;
                for m in 0..LAB_D {
                    LAB_EST[m] += lab_s(b + m as i64);
                }
            }
            let inv = 1.0 / (n as f64);
            for m in 0..LAB_D {
                LAB_EST[m] *= inv * (LAB_WIN[m] as f64);
            }
        } else {
            for i in 0..n {
                let u: f32 = match method {
                    1 => ((i as f32) + rng01()) / (n as f32),
                    // f64: con N grande, la fase de Kronecker en f32 perdería la
                    // estructura de baja discrepancia.
                    2 => fract64(rot + (i as f64) * LAB_GOLDEN64) as f32,
                    _ => rng01(),
                };
                let k = roundi(tri_inv(u) * (a as f32));
                let b = f0 as i64 + k;
                for m in 0..LAB_D {
                    LAB_EST[m] += lab_s(b + m as i64);
                }
            }
            let inv = 1.0 / (n as f64);
            for m in 0..LAB_D {
                LAB_EST[m] *= inv * (LAB_WIN[m] as f64);
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn lab_rms() -> f32 {
    unsafe {
        let mut s = 0.0f64;
        for m in 0..LAB_D {
            let d = LAB_EST[m] - LAB_TGT[m];
            s += d * d;
        }
        sqrtf((s / (LAB_D as f64)) as f32)
    }
}

#[inline]
fn band_filter(i: usize, x: f32) -> f32 {
    unsafe {
        if ABER <= 0.0 {
            return x;
        }
        match VOICES[i].band {
            0 => {
                VOICES[i].lp += A_LOW * (x - VOICES[i].lp);
                VOICES[i].lp
            }
            2 => {
                VOICES[i].lp += A_HIGH * (x - VOICES[i].lp);
                x - VOICES[i].lp
            }
            _ => x,
        }
    }
}

// `offset_sec` permite que el click de la onda siga siendo el punto de inicio
// de Original. Al activar se vacían voces granulares para que no haya mezcla
// escondida entre ambos modelos de fuente.
#[no_mangle]
pub extern "C" fn set_direct(on: u32, offset_sec: f32) {
    unsafe {
        DIRECT_ON = if on == 0 { 0 } else { 1 };
        if DIRECT_ON == 1 {
            let maxp = SAMPLE_LEN.saturating_sub(2) as f32;
            DIRECT_POS = if offset_sec.is_finite() { clampf(offset_sec * SOURCE_SR, 0.0, maxp) } else { 0.0 };
            DIRECT_TONE = 0.0;
            DIRECT_PHASE = 0.0;
            reset_runtime_state();
        }
    }
}

#[inline]
fn modulation_value() -> f32 {
    unsafe {
        match MOD_MODE {
            1 => {
                // Triangular bipolar, deliberadamente sin sin/cos para no_std.
                let p = MOD_PHASE;
                (if p < 0.5 { p * 4.0 - 1.0 } else { 3.0 - p * 4.0 }) * MOD_DEPTH
            }
            2 => (MOD_ENV * 2.0 - 1.0) * MOD_DEPTH,
            _ => 0.0,
        }
    }
}

#[inline]
fn material_sample(i: usize, x: f32) -> f32 {
    unsafe {
        let a = MATERIAL_AMOUNT;
        let shaped = match MATERIAL {
            // Metal: el diferencial enfatiza bordes y parciales altos.
            1 => { let d = x - VOICES[i].tone; VOICES[i].tone += 0.12 * d; x + d * 1.6 },
            // Madera: resonador medio cálido, con una pequeña parte directa.
            2 => { VOICES[i].tone += 0.045 * (x - VOICES[i].tone); VOICES[i].tone * 0.86 + x * 0.14 },
            // Cristal: conserva transitorios brillantes y reduce el cuerpo.
            // Ganancia aproximadamente unitaria: color, no más saturación.
            3 => { let d = x - VOICES[i].tone; VOICES[i].tone += 0.04 * d; x * 0.90 + d * 0.65 },
            // Agua: filtro suave y una ondulación lenta por la edad del rayo.
            4 => { VOICES[i].tone += 0.09 * (x - VOICES[i].tone); x * (0.82 + 0.18 * tri_inv((VOICES[i].age * 0.00037) % 1.0)) + VOICES[i].tone * 0.35 },
            // Plasma: no linealidad contenida, rica sin convertir el bus en ruido.
            5 => x + (x - x * x * x) * 0.75,
            _ => x,
        };
        x + (shaped - x) * a
    }
}

#[inline]
fn direct_material(x: f32) -> f32 {
    unsafe {
        let a = MATERIAL_AMOUNT;
        let shaped = match MATERIAL {
            1 => { let d = x - DIRECT_TONE; DIRECT_TONE += 0.12 * d; x + d * 1.6 },
            2 => { DIRECT_TONE += 0.045 * (x - DIRECT_TONE); DIRECT_TONE * 0.86 + x * 0.14 },
            3 => { let d = x - DIRECT_TONE; DIRECT_TONE += 0.04 * d; x * 0.90 + d * 0.65 },
            4 => { DIRECT_TONE += 0.09 * (x - DIRECT_TONE); x * (0.82 + 0.18 * tri_inv(DIRECT_PHASE)) + DIRECT_TONE * 0.35 },
            5 => x + (x - x * x * x) * 0.75,
            _ => x,
        };
        x + (shaped - x) * a
    }
}

#[inline]
fn ray_reflections(l: f32, r: f32) -> (f32, f32) {
    unsafe {
        // Mantener el historial reciente para una activación limpia, pero no
        // recorrer ni filtrar cuatro trayectorias cuando la cámara está seca.
        // Esto convierte el bypass de Reverb en dos escrituras y un índice.
        if RAY_REVERB_WET <= 0.0 {
            RAY_L[RAY_W] = l;
            RAY_R[RAY_W] = r;
            RAY_W = (RAY_W + 1) % RAY_CAP;
            return (l, r);
        }
        // Cámara de rayos independiente: sus trayectorias no contaminan Delay.
        let paths = [0.017, 0.029, 0.043, 0.071];
        let gains = [0.44, 0.31, 0.22, 0.15];
        let mut ray_l = 0.0;
        let mut ray_r = 0.0;
        for k in 0..4 {
            let d = ((paths[k] * OUTPUT_SR) as usize).min(RAY_CAP - 1);
            let p = (RAY_W + RAY_CAP - d) % RAY_CAP;
            let cross = if k & 1 == 0 { 0.18 } else { 0.42 };
            ray_l += (RAY_L[p] * (1.0 - cross) + RAY_R[p] * cross) * gains[k];
            ray_r += (RAY_R[p] * (1.0 - cross) + RAY_L[p] * cross) * gains[k];
        }
        let absorb = match MATERIAL { 1 => 0.30, 2 => 0.055, 3 => 0.20, 4 => 0.11, 5 => 0.38, _ => 0.16 };
        RAY_TONE_L += absorb * (ray_l - RAY_TONE_L);
        RAY_TONE_R += absorb * (ray_r - RAY_TONE_R);
        if MATERIAL == 1 || MATERIAL == 5 { ray_l += (ray_l - RAY_TONE_L) * MATERIAL_AMOUNT; ray_r += (ray_r - RAY_TONE_R) * MATERIAL_AMOUNT; }
        if MATERIAL == 2 || MATERIAL == 4 { ray_l = ray_l * (1.0 - MATERIAL_AMOUNT * 0.55) + RAY_TONE_L * MATERIAL_AMOUNT * 0.55; ray_r = ray_r * (1.0 - MATERIAL_AMOUNT * 0.55) + RAY_TONE_R * MATERIAL_AMOUNT * 0.55; }
        // Sin wet no hay realimentación: reactivar la cámara nunca recupera
        // una cola escondida; solo parte del historial directo reciente.
        let feedback = RAY_REVERB_WET * (0.18 + RAY_REVERB_WET * 0.24);
        RAY_L[RAY_W] = l + ray_l * feedback;
        RAY_R[RAY_W] = r + ray_r * feedback;
        RAY_W = (RAY_W + 1) % RAY_CAP;
        (l + (ray_l - l) * RAY_REVERB_WET, r + (ray_r - r) * RAY_REVERB_WET)
    }
}

#[inline]
fn spatial_effects(l: f32, r: f32) -> (f32, f32) {
    unsafe {
        let cap = ((OUTPUT_SR * 1.9) as usize).min(DELAY_CAP - 1).max(1);
        let delay_on = DELAY_WET > 0.0;
        let chorus_on = CHORUS_WET > 0.0;
        let flanger_on = FLANGER_WET > 0.0;

        // Bypass barato que conserva una ventana de audio reciente. Antes se
        // calculaban tres taps y dos LFO por muestra aunque los tres wet fueran
        // cero, que es precisamente el estado inicial y el más habitual.
        if !delay_on && !chorus_on && !flanger_on {
            DELAY_L[DELAY_W] = l;
            DELAY_R[DELAY_W] = r;
            DELAY_W = (DELAY_W + 1) % cap;
            return (l, r);
        }

        let delay = ((DELAY_TIME * OUTPUT_SR) as usize).min(cap - 1);
        let read = (DELAY_W + cap - delay) % cap;
        let dl = DELAY_L[read];
        let dr = DELAY_R[read];
        // El feedback pertenece solo al Delay. Antes también realimentaba el
        // buffer con Delay a 0 % si Chorus/Flanger estaban activos, creando una
        // resonancia inesperada y, con algunas fuentes, el pitido denunciado.
        let feedback = if delay_on { DELAY_FB } else { 0.0 };
        DELAY_L[DELAY_W] = l + dl * feedback;
        DELAY_R[DELAY_W] = r + dr * feedback;

        let mut cl = l;
        let mut crr = r;
        if chorus_on {
            let tri = if CHORUS_PHASE < 0.5 { CHORUS_PHASE * 2.0 } else { 2.0 - CHORUS_PHASE * 2.0 };
            let cdelay = ((0.014 + tri * CHORUS_DEPTH) * OUTPUT_SR) as usize;
            let cr = (DELAY_W + cap - cdelay.min(cap - 1)) % cap;
            cl = DELAY_L[cr];
            crr = DELAY_R[cr];
            CHORUS_PHASE += CHORUS_RATE / OUTPUT_SR;
            if CHORUS_PHASE >= 1.0 { CHORUS_PHASE -= 1.0; }
        }
        // Flanger: dos rutas casi idénticas interfieren; el peine se desplaza
        // porque la superficie virtual se mueve lentamente.
        let mut fll = l;
        let mut frr = r;
        if flanger_on {
            let ftri = if FLANGER_PHASE < 0.5 { FLANGER_PHASE * 2.0 } else { 2.0 - FLANGER_PHASE * 2.0 };
            let fdelay = ((0.0006 + ftri * FLANGER_DEPTH) * OUTPUT_SR) as usize;
            let fp = (DELAY_W + cap - fdelay.min(cap - 1)) % cap;
            fll = DELAY_L[fp];
            frr = DELAY_R[fp];
            FLANGER_PHASE += FLANGER_RATE / OUTPUT_SR;
            if FLANGER_PHASE >= 1.0 { FLANGER_PHASE -= 1.0; }
        }
        DELAY_W = (DELAY_W + 1) % cap;
        (l + (dl - l) * DELAY_WET + (cl - l) * CHORUS_WET + (fll - l) * FLANGER_WET,
         r + (dr - r) * DELAY_WET + (crr - r) * CHORUS_WET + (frr - r) * FLANGER_WET)
    }
}

#[inline]
fn phaser(l: f32, r: f32) -> (f32, f32) {
    unsafe {
        if PHASER_WET <= 0.0 { return (l, r); }
        let tri = if PHASER_PHASE < 0.5 { PHASER_PHASE * 2.0 } else { 2.0 - PHASER_PHASE * 2.0 };
        let a = clampf(0.08 + tri * PHASER_DEPTH * 0.78, 0.02, 0.9);
        let mut yl = l;
        let mut yr = r;
        for i in 0..4 {
            let nl = -a * yl + PH_L[i]; PH_L[i] = yl + a * nl; yl = nl;
            let nr = -a * yr + PH_R[i]; PH_R[i] = yr + a * nr; yr = nr;
        }
        PHASER_PHASE += PHASER_RATE / OUTPUT_SR;
        if PHASER_PHASE >= 1.0 { PHASER_PHASE -= 1.0; }
        (l + (yl - l) * PHASER_WET, r + (yr - r) * PHASER_WET)
    }
}

#[inline]
fn resonator(l: f32, r: f32) -> (f32, f32) {
    unsafe {
        if RES_WET <= 0.0 { return (l, r); }
        let d = ((OUTPUT_SR / RES_FREQ) as usize).min(RES_CAP - 1).max(1);
        let p = (RES_W + RES_CAP - d) % RES_CAP;
        let rl = RES_L[p]; let rr = RES_R[p];
        RES_L[RES_W] = l + rl * RES_DECAY;
        RES_R[RES_W] = r + rr * RES_DECAY;
        RES_W = (RES_W + 1) % RES_CAP;
        (l + (rl - l) * RES_WET, r + (rr - r) * RES_WET)
    }
}

#[inline]
fn log_push(off_sec: f32, band: u8, ratio: f32) {
    unsafe {
        let w = (SLOG_W as usize) % SLOG_CAP;
        SLOG[w] = off_sec;
        SLOG_B[w] = band as f32;
        SLOG_S[w] = ratio;
        SLOG_W = SLOG_W.wrapping_add(1);
    }
}

fn alloc_voice() -> usize {
    unsafe {
        ensure_voice_init();
        if NFREE > 0 && NACTIVE < RAY_POPULATION {
            NFREE -= 1;
            let slot = FREE[NFREE] as usize;
            ACTIVE[NACTIVE] = slot as u16;
            NACTIVE += 1;
            return slot;
        }
        // Una poblacion simulada llena debe permanecer estable hasta que un
        // rayo termine. Robar/reiniciar a grain rates altos mantendria todas
        // las Hann cerca de fase cero y produciria renders practicamente mudos.
        if NACTIVE >= RAY_POPULATION {
            return MAX_VOICES;
        }
        // Sin slots libres: robar la voz más APAGADA (fase más avanzada → la cola
        // de la Hann está cerca de 0) en vez de la más vieja → robo sin click.
        let mut best = 0usize;
        let mut best_ph = -1.0f32;
        for k in 0..NACTIVE {
            let v = ACTIVE[k] as usize;
            let ph = VOICES[v].age * VOICES[v].inv_dur;
            if ph > best_ph {
                best_ph = ph;
                best = k;
            }
        }
        ACTIVE[best] as usize // ya está en la lista activa; se reutiliza en sitio
    }
}

// Min-heap acotado: seleccion parcial O(N log 36), sin ordenar la poblacion.
// El grid uniforme aporta una estimacion local de densidad en O(N).
fn select_top36() {
    unsafe {
        let old = TOP_ID;
        let old_count = TOP_COUNT;
        for k in 0..MIX_COUNT { VOICES[MIX_INDEX[k] as usize].mix_target = 0.0; }
        for b in 0..GRID_BINS { GRID_COUNT[b] = 0; }
        let span = (SAMPLE_LEN.saturating_sub(1) as f32).max(1.0);
        for k in 0..NACTIVE {
            let i = ACTIVE[k] as usize;
            let b = (((VOICES[i].pos / span) * GRID_BINS as f32) as usize).min(GRID_BINS - 1);
            GRID_COUNT[b] = GRID_COUNT[b].saturating_add(1);
        }
        TOP_COUNT = 0;
        for k in 0..NACTIVE {
            let i = ACTIVE[k] as usize;
            let v = VOICES[i];
            let ph = clampf(v.age * v.inv_dur, 0.0, 1.0);
            let b = (((v.pos / span) * GRID_BINS as f32) as usize).min(GRID_BINS - 1);
            let local = GRID_COUNT[b] as f32
                + if b > 0 { GRID_COUNT[b - 1] as f32 * 0.5 } else { 0.0 }
                + if b + 1 < GRID_BINS { GRID_COUNT[b + 1] as f32 * 0.5 } else { 0.0 };
            let source = if SAMPLE_LEN >= 2 { core_sample_at(&SAMPLE[..SAMPLE_LEN], v.pos) } else { 0.0 };
            let energy = if source < 0.0 { -source } else { source };
            let envelope = 1.0 - (ph * 2.0 - 1.0).abs();
            // La inercia introduce histeresis barata: conservar un candidato
            // debe superar a uno nuevo por menos margen, sin crear mas voces.
            let retained_bonus = if TOP_MASK[i] { TOP_INERTIA * 0.25 } else { 0.0 };
            let score = energy * 0.55 + envelope * 0.35 + (1.0 / (1.0 + local)) * 0.10 + retained_bonus;
            if TOP_COUNT < TOP_K {
                let mut p = TOP_COUNT;
                TOP_INDEX[p] = i as u16; TOP_SCORE[p] = score; TOP_COUNT += 1;
                while p > 0 {
                    let parent = (p - 1) / 2;
                    if TOP_SCORE[parent] <= TOP_SCORE[p] { break; }
                    TOP_SCORE.swap(parent, p); TOP_INDEX.swap(parent, p); p = parent;
                }
            } else if score > TOP_SCORE[0] {
                TOP_SCORE[0] = score; TOP_INDEX[0] = i as u16;
                let mut p = 0usize;
                loop {
                    let l = p * 2 + 1; if l >= TOP_K { break; }
                    let r = l + 1;
                    let c = if r < TOP_K && TOP_SCORE[r] < TOP_SCORE[l] { r } else { l };
                    if TOP_SCORE[p] <= TOP_SCORE[c] { break; }
                    TOP_SCORE.swap(p, c); TOP_INDEX.swap(p, c); p = c;
                }
            }
        }
        for i in 0..MAX_RAYS { TOP_MASK[i] = false; }
        for k in 0..TOP_COUNT { TOP_MASK[TOP_INDEX[k] as usize] = true; }
        for k in 0..TOP_COUNT { TOP_ID[k] = VOICES[TOP_INDEX[k] as usize].id; }
        for k in 0..TOP_COUNT {
            let i = TOP_INDEX[k] as usize;
            VOICES[i].mix_target = 1.0;
            let mut present = false;
            for j in 0..MIX_COUNT { if MIX_INDEX[j] as usize == i { present = true; break; } }
            if !present && MIX_COUNT < MAX_MIX { MIX_INDEX[MIX_COUNT] = i as u16; MIX_COUNT += 1; }
        }
        let mut retained = 0usize;
        for k in 0..TOP_COUNT {
            for j in 0..old_count {
                if old[j] == TOP_ID[k] { retained += 1; break; }
            }
        }
        // Medimos unicamente transiciones Top-36 completas: retained / 36.
        // El calentamiento y los conjuntos parciales no sesgan la poblacion.
        if old_count == TOP_K && TOP_COUNT == TOP_K {
            TOP_RETENTION = retained as f32 / TOP_K as f32;
            TOP_RETENTION_SUM += TOP_RETENTION as f64;
            TOP_TRANSITIONS = TOP_TRANSITIONS.wrapping_add(1);
            TOP_ENTRIES = TOP_ENTRIES.wrapping_add((TOP_COUNT - retained) as u32);
            TOP_EXITS = TOP_EXITS.wrapping_add((old_count - retained) as u32);
            if retained != old_count || retained != TOP_COUNT {
                TOP_CHANGES = TOP_CHANGES.wrapping_add(1);
            }
        }

        // Descriptores fisicos del conjunto audible. Se acumulan por update
        // para que cada render produzca una unica fila comparable.
        if TOP_COUNT == TOP_K {
            let mut pitches = [0.0f32; TOP_K];
            let mut pitch_sum = 0.0f64;
            let mut age_sum = 0.0f64;
            let mut energy_sum = 0.0f64;
            let mut duration_sum = 0.0f64;
            let sr_ratio = (SOURCE_SR / OUTPUT_SR).max(0.000_001);
            for k in 0..TOP_K {
                let v = VOICES[TOP_INDEX[k] as usize];
                let pitch = v.step / sr_ratio;
                let ph = clampf(v.age * v.inv_dur, 0.0, 1.0);
                let energy = if SAMPLE_LEN >= 2 {
                    let x = core_sample_at(&SAMPLE[..SAMPLE_LEN], v.pos);
                    if x < 0.0 { -x } else { x }
                } else { 0.0 };
                pitches[k] = pitch;
                pitch_sum += pitch as f64;
                age_sum += (v.age / OUTPUT_SR) as f64;
                energy_sum += energy as f64;
                duration_sum += ((1.0 - ph) / (v.inv_dur * OUTPUT_SR).max(0.000_001)) as f64;
            }
            // Insertion sort de 36 valores; nunca se ordena la poblacion N.
            for i in 1..TOP_K {
                let value = pitches[i];
                let mut j = i;
                while j > 0 && pitches[j - 1] > value { pitches[j] = pitches[j - 1]; j -= 1; }
                pitches[j] = value;
            }
            TOP_METRIC_TICKS = TOP_METRIC_TICKS.wrapping_add(1);
            TOP_PITCH_MEAN_SUM += pitch_sum / TOP_K as f64;
            TOP_PITCH_MEDIAN_SUM += ((pitches[TOP_K / 2 - 1] + pitches[TOP_K / 2]) * 0.5) as f64;
            TOP_AGE_MEAN_SUM += age_sum / TOP_K as f64;
            TOP_ENERGY_MEAN_SUM += energy_sum / TOP_K as f64;
            TOP_DURATION_MEAN_SUM += duration_sum / TOP_K as f64;
        }
    }
}

// Coloca un grano (usado por granos nuevos y rebotes). Decide octava, transposición y paneo.
fn place(pos: f32, band: u8, depth: u32) {
    unsafe {
        let dur_samp = GRAIN_DUR * OUTPUT_SR;
        if dur_samp < 1.0 {
            return;
        }
        let detune = 1.0 + (rng01() - 0.5) * DETUNE; // micro-detune lush (batidos entre granos)
        // octava × transposición × grado microtonal/tecla × detune: la retícula
        // exacta + el detune ⇒ enjambre alrededor de cada grado, no comb estático.
        // Las teclas sostenidas (piano) tienen prioridad: mientras haya alguna
        // pulsada, cada grano toca una de esas notas en vez del grado de
        // Microtonal/Voicing — igual que en el VST, tocar anula la textura fija.
        let ratio = if KEYS_LEN > 0 { key_ratio() } else { scale_ratio() };
        let mod_pitch = if MOD_TARGET == 2 { 1.0 + modulation_value() * 0.35 } else { 1.0 };
        let plasma_detune = if MATERIAL == 5 { 1.0 + (rng01() - 0.5) * MATERIAL_AMOUNT * 0.04 } else { 1.0 };
        let step = (if rng01() < OCT { 2.0 } else { 1.0 }) * PITCH_STEP * ratio * detune * mod_pitch * plasma_detune
            * (SOURCE_SR / OUTPUT_SR);
        let pan = (rng01() * 2.0 - 1.0) * WIDTH; // paneo aleatorio según el ancho
        let panl = sqrtf((1.0 - pan) * 0.5); // equal-power
        let panr = sqrtf((1.0 + pan) * 0.5);
        let maxp = SAMPLE_LEN.saturating_sub(2) as f32;
        let readable_span = if step.is_finite() { (dur_samp - 1.0).max(0.0) * step.max(0.0) } else { maxp };
        let start = clampf(pos, 0.0, (maxp - readable_span).max(0.0));
        let slot = alloc_voice();
        if slot == MAX_VOICES { return; }
        VOICES[slot] = Voice {
            active: true,
            id: NEXT_RAY_ID,
            mix_gain: 0.0,
            mix_target: 0.0,
            pos: start,
            age: 0.0,
            inv_dur: 1.0 / dur_samp,
            gain: GAIN,
            band,
            lp: 0.0,
            tone: 0.0,
            depth,
            step,
            panl,
            panr,
        };
        NEXT_RAY_ID = NEXT_RAY_ID.wrapping_add(1).max(1);
        log_push(start / SOURCE_SR, band, ratio);
        SPAWN_COUNT = SPAWN_COUNT.wrapping_add(1);
    }
}

fn spawn() {
    unsafe {
        if SAMPLE_LEN < 2 {
            return;
        }
        let r = rng01();
        let band: u8 = if r < 0.4 {
            0
        } else if r < 0.8 {
            1
        } else {
            2
        };
        let scale = if ABER <= 0.0 {
            1.0
        } else {
            match band {
                0 => 1.0 + ABER * 2.2,
                2 => clampf(1.0 - ABER * 0.72, 0.08, 1.0),
                _ => 1.0,
            }
        };
        let span = (SAMPLE_LEN as f32) / SOURCE_SR;
        // Foco efectivo: en ambient, un foco de la constelación (ponderado por peso),
        // con apertura más cerrada a escala fina (auto-similar). Si no, el FOCUS
        // único de siempre con su autoevolución acotada por FEEDBACK.
        let (eff_focus, eff_ap);
        let fsel = if AMB_ON == 1 { pick_focus() } else { -1 };
        if fsel >= 0 {
            let fi = fsel as usize;
            let level = (AMB_DEPTH as i32 - FDEPTH[fi] as i32).max(0) as u32;
            eff_focus = clampf(FPOS[fi], 0.0, span);
            let m = if MOD_TARGET == 1 { 1.0 + modulation_value() } else { 1.0 };
            eff_ap = APERTURE * (1.0 + FEEDBACK * ENV * 0.8) * powf_i(0.7, level) * m.max(0.05);
        } else {
            eff_focus = clampf(FOCUS + (EVO - 0.5) * FEEDBACK * span * 0.45, 0.0, span);
            let m = if MOD_TARGET == 1 { 1.0 + modulation_value() } else { 1.0 };
            eff_ap = APERTURE * (1.0 + FEEDBACK * ENV * 0.8) * m.max(0.05);
        }
        let mut off_sec = eff_focus + tri_inv(next_u()) * eff_ap * scale;
        // Trazado inverso: rejection ∝ energía → los rayos caen donde hay señal.
        if SMART == 1 {
            let mut tries = 0;
            while tries < 6 {
                if energy_at(off_sec) >= EMAX * rng01() {
                    break;
                }
                off_sec = eff_focus + tri_inv(next_u()) * eff_ap * scale;
                tries += 1;
            }
        }
        let maxp = (SAMPLE_LEN - 2) as f32;
        let pos = clampf(off_sec * SOURCE_SR, 0.0, maxp);
        place(pos, band, BOUNCES);
    }
}

// Mantiene la simulacion en N candidatos con independencia de las voces de
// audio y del grain rate. Los candidatos que terminan se regeneran; el Top-36
// es la unica frontera que alcanza el mezclador granular.
fn fill_ray_population() {
    unsafe {
        if SAMPLE_LEN < 2 || DIRECT_ON == 1 || GRAIN_RATE <= 0.0 { return; }
        let initial_fill = NACTIVE == 0;
        let mut born = 0usize;
        while NACTIVE < RAY_POPULATION {
            let before = NACTIVE;
            spawn();
            if initial_fill && NACTIVE > before {
                // Distribuir las fases evita que 2.000 Hann nazcan y mueran a
                // la vez. Es el equivalente a arrancar un campo estacionario,
                // no una ráfaga sincronizada.
                let i = ACTIVE[NACTIVE - 1] as usize;
                let dur_samples = 1.0 / VOICES[i].inv_dur.max(0.000_001);
                let phase = (born as f32 + 0.5) / RAY_POPULATION as f32;
                VOICES[i].age = phase * dur_samples;
                VOICES[i].pos += VOICES[i].step * VOICES[i].age;
                born += 1;
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn process(frames: usize) {
    unsafe {
        let n = if frames > BLOCK { BLOCK } else { frames };
        if n == 0 {
            return;
        }
        let maxp = if SAMPLE_LEN >= 2 {
            (SAMPLE_LEN - 2) as f32
        } else {
            0.0
        };
        // La constelación de focos avanza una vez por bloque (deriva lenta).
        update_foci(n as f32 / OUTPUT_SR);
        fill_ray_population();
        RAY_UPDATE_ACC += n as f32;
        let update_frames = (OUTPUT_SR / RAY_UPDATE_HZ).max(1.0);
        if TOP_COUNT == 0 || RAY_UPDATE_ACC >= update_frames {
            select_top36();
            if RAY_UPDATE_ACC >= update_frames { RAY_UPDATE_ACC -= update_frames; }
        }
        for f in 0..n {
            MOD_PHASE += MOD_RATE / OUTPUT_SR;
            if MOD_PHASE >= 1.0 { MOD_PHASE -= 1.0; }
            if DIRECT_ON == 1 {
                // Original = un rayo continuo de la fuente. Comparte desde aquí
                // hasta la salida toda la acústica del modo granular.
                let raw = if SAMPLE_LEN >= 2 { core_sample_at(&SAMPLE[..SAMPLE_LEN], DIRECT_POS) } else { 0.0 };
                let motion_gain = if MOD_TARGET == 0 { (1.0 + modulation_value() * 0.55).max(0.0) } else { 1.0 };
                let src = direct_material(raw) * motion_gain;
                let (fl, fr) = FILTER.process(src * MASTER, src * MASTER);
                let (rl0, rr0) = ray_reflections(fl, fr);
                let (dl, dr) = spatial_effects(rl0, rr0);
                let (pl, pr) = phaser(dl, dr);
                let (ol, or) = resonator(pl, pr);
                let driven_l = ol + (soft(ol * (1.0 + DRIVE * 7.0)) - ol) * DRIVE;
                let driven_r = or + (soft(or * (1.0 + DRIVE * 7.0)) - or) * DRIVE;
                let (rl, rr) = REVERB.process(driven_l, driven_r);
                let (yl, yr) = DC.process(rl, rr);
                OUTL[f] = soft(yl);
                OUTR[f] = soft(yr);
                let pitch_motion = if MOD_TARGET == 2 { 1.0 + modulation_value() * 0.35 } else { 1.0 };
                DIRECT_POS += (SOURCE_SR / OUTPUT_SR) * PITCH_STEP * pitch_motion.max(0.05);
                DIRECT_PHASE += 0.00037;
                if DIRECT_PHASE >= 1.0 { DIRECT_PHASE -= 1.0; }
                if SAMPLE_LEN >= 2 && DIRECT_POS > maxp { DIRECT_POS -= (SAMPLE_LEN - 2) as f32; }
                continue;
            }
            let mod_density = if MOD_TARGET == 0 { 1.0 + modulation_value() } else { 1.0 };
            let mut accl = 0.0f32;
            let mut accr = 0.0f32;
            let mut k = 0usize;
            while k < MIX_COUNT {
                let i = MIX_INDEX[k] as usize;
                if !VOICES[i].active { k += 1; continue; }
                let ph = VOICES[i].age * VOICES[i].inv_dur;
                if ph < 1.0 {
                    let fade_step = 1.0 / (CROSSFADE_S * OUTPUT_SR).max(1.0);
                    if VOICES[i].mix_gain < VOICES[i].mix_target {
                        VOICES[i].mix_gain = (VOICES[i].mix_gain + fade_step).min(VOICES[i].mix_target);
                    } else if VOICES[i].mix_gain > VOICES[i].mix_target {
                        VOICES[i].mix_gain = (VOICES[i].mix_gain - fade_step).max(VOICES[i].mix_target);
                    }
                    let raw = core_sample_at(&SAMPLE[..SAMPLE_LEN], VOICES[i].pos);
                    let s = material_sample(i, band_filter(i, raw)) * core_win_at(&WINDOW, ph) * VOICES[i].gain * VOICES[i].mix_gain * TOP_MIX_COMP;
                    accl += s * VOICES[i].panl;
                    accr += s * VOICES[i].panr;
                    VOICES[i].pos += VOICES[i].step;
                    VOICES[i].age += mod_density.max(0.05);
                }
                k += 1;
            }
            let (fl, fr) = FILTER.process(accl * MASTER, accr * MASTER);
            let (rl0, rr0) = ray_reflections(fl, fr);
            let (dl, dr) = spatial_effects(rl0, rr0);
            let (pl, pr) = phaser(dl, dr);
            let (ol, or) = resonator(pl, pr);
            let driven_l = ol + (soft(ol * (1.0 + DRIVE * 7.0)) - ol) * DRIVE;
            let driven_r = or + (soft(or * (1.0 + DRIVE * 7.0)) - or) * DRIVE;
            let (rl, rr) = REVERB.process(driven_l, driven_r);
            let (yl, yr) = DC.process(rl, rr);
            OUTL[f] = soft(yl);
            OUTR[f] = soft(yr);
        }

        if DIRECT_ON == 0 {
            // Los candidatos no audibles avanzan una vez por bloque: O(N).
            // Sólo el Top-36 ha recorrido el DSP muestra a muestra arriba.
            let density_step = if MOD_TARGET == 0 { (1.0 + modulation_value()).max(0.05) } else { 1.0 };
            let mut k = 0usize;
            while k < NACTIVE {
                let i = ACTIVE[k] as usize;
                if VOICES[i].mix_gain <= 0.0 && VOICES[i].mix_target <= 0.0 {
                    VOICES[i].pos += VOICES[i].step * n as f32;
                    VOICES[i].age += density_step * n as f32;
                }
                if VOICES[i].age * VOICES[i].inv_dur >= 1.0 {
                    let dep = VOICES[i].depth;
                    let bnd = VOICES[i].band;
                    let endpos = VOICES[i].pos;
                    VOICES[i].active = false;
                    TOP_MASK[i] = false;
                    VOICES[i].mix_gain = 0.0;
                    VOICES[i].mix_target = 0.0;
                    NACTIVE -= 1;
                    ACTIVE[k] = ACTIVE[NACTIVE];
                    FREE[NFREE] = i as u16;
                    NFREE += 1;
                    if dep > 0 && rng01() < REFL {
                        let jitter = (rng01() - 0.5) * 0.1 * SOURCE_SR;
                        let cpos = clampf(endpos + jitter, 0.0, maxp);
                        place(cpos, bnd, dep - 1);
                    }
                    continue;
                }
                k += 1;
            }
            let mut m = 0usize;
            while m < MIX_COUNT {
                let i = MIX_INDEX[m] as usize;
                if !VOICES[i].active || (VOICES[i].mix_gain <= 0.0 && VOICES[i].mix_target <= 0.0) {
                    MIX_COUNT -= 1;
                    MIX_INDEX[m] = MIX_INDEX[MIX_COUNT];
                    continue;
                }
                m += 1;
            }
        }

        // La telemetría al terminar el bloque siempre observa la población N.
        fill_ray_population();

        // Envolvente (para el medidor) + autoevolución recursiva.
        let mut s = 0.0f32;
        for f in 0..n {
            let l = if OUTL[f] < 0.0 { -OUTL[f] } else { OUTL[f] };
            let r = if OUTR[f] < 0.0 { -OUTR[f] } else { OUTR[f] };
            s += (l + r) * 0.5;
        }
        let blk = s / (n as f32);
        ENV = ENV * 0.9 + blk * 0.1;
        let coeff = if blk > MOD_ENV { 1.0 / (MOD_ATTACK * OUTPUT_SR).max(1.0) } else { 1.0 / (MOD_RELEASE * OUTPUT_SR).max(1.0) };
        MOD_ENV += (blk - MOD_ENV) * coeff * (n as f32);
        if FEEDBACK > 0.0 {
            // Barrido más lento y menos dependiente del nivel → evoluciona en vez de saltar.
            let step = FEEDBACK * (0.0004 + ENV * 0.0018);
            EVO += EVO_DIR * step;
            if EVO >= 1.0 {
                EVO = 1.0;
                EVO_DIR = -1.0;
            }
            if EVO <= 0.0 {
                EVO = 0.0;
                EVO_DIR = 1.0;
            }
        }
    }
}
