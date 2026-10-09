//! A tiny synthesiser: every sound in the game is generated here at start-up
//! (noise bursts, swept tones, filtered formants), so the game ships without
//! any audio files.

use std::f32::consts::TAU;

pub const RATE: u32 = 44_100;

/// A mono buffer of samples in -1..1.
#[derive(Clone)]
pub struct Buf(pub Vec<f32>);

/// Deterministic white noise.
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Self {
        Self(seed.wrapping_mul(2_654_435_761).max(1))
    }
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// One-pole low-pass filter.
pub struct Lp {
    y: f32,
}

impl Lp {
    pub fn new() -> Self {
        Self { y: 0.0 }
    }
    pub fn run(&mut self, x: f32, cutoff: f32) -> f32 {
        let a = 1.0 - (-TAU * cutoff.max(10.0) / RATE as f32).exp();
        self.y += a * (x - self.y);
        self.y
    }
}

/// One-pole high-pass filter.
pub struct Hp {
    x_prev: f32,
    y: f32,
}

impl Hp {
    pub fn new() -> Self {
        Self { x_prev: 0.0, y: 0.0 }
    }
    pub fn run(&mut self, x: f32, cutoff: f32) -> f32 {
        let rc = 1.0 / (TAU * cutoff.clamp(10.0, RATE as f32 * 0.45));
        let dt = 1.0 / RATE as f32;
        let alpha = rc / (rc + dt);
        let y = alpha * (self.y + x - self.x_prev);
        self.x_prev = x;
        self.y = y;
        y
    }
}

/// Resonant band-pass (biquad, constant peak gain).
pub struct Bp {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Bp {
    pub fn new() -> Self {
        Self {
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
    pub fn run(&mut self, x: f32, freq: f32, q: f32) -> f32 {
        let w = TAU * freq.clamp(20.0, RATE as f32 * 0.45) / RATE as f32;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b0 = alpha / a0;
        let b2 = -alpha / a0;
        let a1 = -2.0 * w.cos() / a0;
        let a2 = (1.0 - alpha) / a0;
        let y = b0 * x + b2 * self.x2 - a1 * self.y1 - a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Fills `secs` of audio from `f(t)`.
pub fn render(secs: f32, mut f: impl FnMut(f32) -> f32) -> Buf {
    let n = (secs * RATE as f32) as usize;
    Buf((0..n).map(|i| f(i as f32 / RATE as f32)).collect())
}

/// Exponential decay with time constant `tau`.
pub fn decay(t: f32, tau: f32) -> f32 {
    (-t / tau).exp()
}

/// Rises over `a` seconds, then decays with `tau`.
pub fn env(t: f32, a: f32, tau: f32) -> f32 {
    if t < a {
        t / a
    } else {
        decay(t - a, tau)
    }
}

/// Calculates playback speed multiplier from pitch offset in cents (1200 cents per octave).
#[allow(dead_code)]
pub fn pitch_from_cents(cents: f32) -> f32 {
    2.0_f32.powf(cents / 1200.0)
}

/// Calculates linear gain from decibels (dB).
#[allow(dead_code)]
pub fn gain_from_db(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// A sine oscillator that can change pitch smoothly.
pub struct Osc {
    phase: f32,
}

impl Osc {
    pub fn new() -> Self {
        Self { phase: 0.0 }
    }
    pub fn sine(&mut self, freq: f32) -> f32 {
        self.phase = (self.phase + freq / RATE as f32).fract();
        (self.phase * TAU).sin()
    }
    /// Band-limited-ish sawtooth (a few harmonics), good for voices.
    pub fn saw(&mut self, freq: f32) -> f32 {
        self.phase = (self.phase + freq / RATE as f32).fract();
        let p = self.phase * TAU;
        let mut s = 0.0;
        let harmonics = ((RATE as f32 * 0.4) / freq.max(20.0)).min(24.0) as i32;
        for k in 1..=harmonics.max(1) {
            s += (p * k as f32).sin() / k as f32;
        }
        s * 0.6
    }
    pub fn square(&mut self, freq: f32) -> f32 {
        self.phase = (self.phase + freq / RATE as f32).fract();
        if self.phase < 0.5 {
            1.0
        } else {
            -1.0
        }
    }
}

impl Buf {
    /// Scales so the loudest sample hits `peak`, with a soft clip.
    pub fn normalize(mut self, peak: f32) -> Self {
        let max = self.0.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
        for s in &mut self.0 {
            *s = (*s / max * peak * 1.2).tanh() / 1.2f32.tanh();
        }
        self.fade()
    }

    /// Short fades at both ends so nothing clicks.
    pub fn fade(mut self) -> Self {
        let n = self.0.len();
        let f = 64.min(n / 2);
        for i in 0..f {
            let g = i as f32 / f as f32;
            self.0[i] *= g;
            self.0[n - 1 - i] *= g;
        }
        self
    }

    /// Mixes `other` in, starting `at` seconds in.
    pub fn mix(mut self, other: &Buf, at: f32, gain: f32) -> Self {
        let start = (at * RATE as f32) as usize;
        if self.0.len() < start + other.0.len() {
            self.0.resize(start + other.0.len(), 0.0);
        }
        for (i, s) in other.0.iter().enumerate() {
            self.0[start + i] += s * gain;
        }
        self
    }

    /// A cheap room echo: a few delayed, darker copies.
    pub fn echo(mut self, delay: f32, feedback: f32, taps: usize) -> Self {
        let d = (delay * RATE as f32) as usize;
        let extra = d * taps;
        let n = self.0.len();
        self.0.resize(n + extra, 0.0);
        let mut lp = Lp::new();
        for i in d..self.0.len() {
            let wet = lp.run(self.0[i - d], 2500.0) * feedback;
            self.0[i] += wet;
        }
        self
    }

    /// 16-bit mono WAV file bytes.
    pub fn wav(&self) -> Vec<u8> {
        let data_len = (self.0.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&RATE.to_le_bytes());
        out.extend_from_slice(&(RATE * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for s in &self.0 {
            out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32_000.0) as i16).to_le_bytes());
        }
        out
    }

    /// Low-pass filter the buffer with a one-pole filter at `cutoff` Hz (auditory tunneling).
    pub fn low_pass(&self, cutoff: f32) -> Self {
        let mut lp = Lp::new();
        let filtered = self.0.iter().map(|&s| lp.run(s, cutoff)).collect();
        Buf(filtered)
    }
}

// ---------------------------------------------------------------------------
// Sound recipes
// ---------------------------------------------------------------------------

/// What a gunshot sounds like.
pub struct ShotRecipe {
    /// Low "body" thump frequency.
    pub body: f32,
    /// How long the blast rings.
    pub tail: f32,
    /// Brightness of the blast noise.
    pub bright: f32,
    /// Echo off the surroundings.
    pub echo: f32,
}

/// A low-end sub-bass thump: a pitch-dropping sine wave with fast exponential decay
/// (e.g. 90 Hz dropping to 35 Hz) providing visceral low-end punch.
pub fn sub_thump(start_hz: f32, end_hz: f32, decay_tau: f32, len: f32) -> Buf {
    let mut o = Osc::new();
    render(len, |t| {
        let drop = decay(t, decay_tau * 0.45);
        let freq = end_hz + (start_hz - end_hz) * drop;
        o.sine(freq) * decay(t, decay_tau)
    })
    .normalize(0.95)
}

pub fn gunshot_reverb(r: &ShotRecipe, seed: u32, indoor: bool) -> Buf {
    let mut n = Noise::new(seed);
    let mut hp = Hp::new();
    let mut bp_snap = Bp::new();
    let mut lp_body = Lp::new();
    let mut bp_bark = Bp::new();
    let mut lp_rumble = Lp::new();
    let mut osc_thump = Osc::new();
    let mut osc_sub = Osc::new();

    let tail_mult = if indoor { 0.75 } else { 1.25 };
    let len = if indoor {
        (r.tail * 3.6 + 0.16).max(0.32)
    } else {
        (r.tail * 5.6 + 0.42).max(0.68)
    };

    let b = render(len, |t| {
        let x = n.next();

        // 1. Transient supersonic crack & mechanical snap (first 2-5ms)
        let crack_raw = hp.run(x, 2200.0) * decay(t, 0.0028) * 2.2;
        let snap_metal = bp_snap.run(x, (r.bright * 0.9).clamp(2400.0, 7500.0), 4.5)
            * decay(t, 0.0045)
            * 1.8;
        let transient = crack_raw + snap_metal;

        // 2. Concussive body & saturated mid bark
        // Resonant blast swept downward
        let blast_freq = r.bright * (0.3 + 0.7 * decay(t, 0.025));
        let blast = lp_body.run(x, blast_freq) * decay(t, r.tail * 0.95);
        // Resonant explosive bark (tanh saturation)
        let bark_raw = bp_bark.run(x, (r.body * 2.6).clamp(180.0, 900.0), 2.2)
            * decay(t, r.tail * 0.6)
            * 2.5;
        let concussive_body = (blast * 1.5 + bark_raw).tanh();

        // 3. Low-end body & sub-bass thump
        let thump_freq = r.body * (1.0 + 2.8 * decay(t, 0.016));
        let thump = osc_thump.sine(thump_freq) * decay(t, r.tail * 0.85);
        let sub = osc_sub.sine(48.0 + 36.0 * decay(t, 0.04)) * decay(t, r.tail * 1.4) * 0.8;

        // 4. Lingering low rumble
        let rumble = lp_rumble.run(x, 240.0) * decay(t, r.tail * 2.6 * tail_mult) * 0.7;

        transient * 1.1 + concussive_body * 1.4 + thump * 1.0 + sub + rumble
    });

    if indoor {
        // Enclosed room / corridor: tight slapback echo (20-40ms decay, crisp early reflections)
        b.echo(0.024, 0.42, 2)
            .echo(0.038, 0.28, 2)
            .normalize(0.98)
    } else {
        // Open outdoor area: long, rolling sub-bass tail (250-400ms decay)
        let mut n_tail = Noise::new(seed.wrapping_add(888));
        let mut lp_tail = Lp::new();
        let outdoor_tail = render(0.55, |t| {
            let roll = lp_tail.run(n_tail.next(), 220.0 * decay(t, 0.35) + 60.0);
            roll * env(t, 0.03, 0.32) * 1.1
        });
        b.echo(0.085, (r.echo * 0.7).min(0.35), 2)
            .echo(0.18, (r.echo * 0.6).min(0.3), 2)
            .mix(&outdoor_tail, 0.05, 0.75)
            .normalize(0.98)
    }
}

#[allow(dead_code)]
pub fn gunshot(r: &ShotRecipe, seed: u32) -> Buf {
    gunshot_reverb(r, seed, false)
}

/// Gunshot layered with a deep concussive sub-bass thump and acoustic wallop.
pub fn heavy_gunshot_reverb(
    r: &ShotRecipe,
    seed: u32,
    thump_start: f32,
    thump_end: f32,
    indoor: bool,
) -> Buf {
    let shot = gunshot_reverb(r, seed, indoor);
    let thump_decay = if indoor { (r.tail * 0.75).max(0.06) } else { (r.tail * 1.2).max(0.12) };
    let thump_len = if indoor { (r.tail * 2.2 + 0.1).max(0.24) } else { (r.tail * 3.8 + 0.25).max(0.45) };
    let thump = sub_thump(thump_start, thump_end, thump_decay, thump_len);
    // Add extra mechanical concussive slap
    let mut n = Noise::new(seed.wrapping_add(101));
    let mut hp = Hp::new();
    let slap = render(0.08, |t| {
        hp.run(n.next(), 3200.0) * decay(t, 0.003) * 1.4
    });
    shot.mix(&thump, 0.0, 1.2)
        .mix(&slap, 0.0, 0.5)
        .normalize(0.98)
}

#[allow(dead_code)]
pub fn heavy_gunshot(r: &ShotRecipe, seed: u32, thump_start: f32, thump_end: f32) -> Buf {
    heavy_gunshot_reverb(r, seed, thump_start, thump_end, false)
}

/// M1911 Pistol: Crisp metallic slide/action snap transient + punchy pop.
pub fn pistol_shot_reverb(seed: u32, indoor: bool) -> Buf {
    let recipe = ShotRecipe {
        body: 185.0,
        tail: 0.055,
        bright: 5200.0,
        echo: 0.28,
    };
    let shot = gunshot_reverb(&recipe, seed, indoor);
    let action_click = click(3400.0, seed.wrapping_add(201), 0.03);
    shot.mix(&action_click, 0.001, 0.6).normalize(0.96)
}

#[allow(dead_code)]
pub fn pistol_shot(seed: u32) -> Buf {
    pistol_shot_reverb(seed, false)
}

/// Magnum / Revolver: Thunderous, concussive hand-cannon roar with heavy sub thump & metallic ring.
pub fn magnum_shot_reverb(seed: u32, indoor: bool) -> Buf {
    let recipe = ShotRecipe {
        body: 110.0,
        tail: 0.12,
        bright: 3800.0,
        echo: 0.48,
    };
    let shot = heavy_gunshot_reverb(&recipe, seed, 105.0, 36.0, indoor);
    // Cylinder / frame resonant ring overtone
    let ring = bell(&[(2850.0, 0.5, 0.08), (4280.0, 0.3, 0.04)], 0.15);
    shot.mix(&ring, 0.003, 0.35).normalize(0.98)
}

#[allow(dead_code)]
pub fn magnum_shot(seed: u32) -> Buf {
    magnum_shot_reverb(seed, false)
}

/// SMG: Rapid, snappy, tight muzzle crack with crisp mechanical action cycling.
pub fn smg_shot_reverb(seed: u32, indoor: bool) -> Buf {
    let recipe = ShotRecipe {
        body: 220.0,
        tail: 0.042,
        bright: 6200.0,
        echo: 0.22,
    };
    let shot = gunshot_reverb(&recipe, seed, indoor);
    let bolt_click = click(4200.0, seed.wrapping_add(301), 0.025);
    shot.mix(&bolt_click, 0.001, 0.7).normalize(0.95)
}

#[allow(dead_code)]
pub fn smg_shot(seed: u32) -> Buf {
    smg_shot_reverb(seed, false)
}

/// Assault Rifle: Punchy staccato bark with snappy brassy transients and solid low punch.
pub fn rifle_shot_reverb(seed: u32, indoor: bool) -> Buf {
    let recipe = ShotRecipe {
        body: 135.0,
        tail: 0.075,
        bright: 4600.0,
        echo: 0.36,
    };
    let shot = gunshot_reverb(&recipe, seed, indoor);
    let thump = sub_thump(110.0, 45.0, 0.065, 0.2);
    shot.mix(&thump, 0.0, 0.7).normalize(0.97)
}

#[allow(dead_code)]
pub fn rifle_shot(seed: u32) -> Buf {
    rifle_shot_reverb(seed, false)
}

/// Shotgun: Devastating close-range blast, massive low-end wallop, wide acoustic roar.
pub fn shotgun_shot_reverb(seed: u32, indoor: bool) -> Buf {
    let recipe = ShotRecipe {
        body: 72.0,
        tail: 0.16,
        bright: 2800.0,
        echo: 0.52,
    };
    let shot = heavy_gunshot_reverb(&recipe, seed, 92.0, 28.0, indoor);
    let spread_crack = render(0.06, {
        let mut n = Noise::new(seed.wrapping_add(401));
        let mut bp = Bp::new();
        move |t| bp.run(n.next(), 1800.0, 1.2) * decay(t, 0.008) * 1.8
    });
    shot.mix(&spread_crack, 0.0, 0.8).normalize(0.98)
}

#[allow(dead_code)]
pub fn shotgun_shot(seed: u32) -> Buf {
    shotgun_shot_reverb(seed, false)
}

/// LMG: Heavy sustained hammer, deep thumping punch and echoing rattle.
pub fn lmg_shot_reverb(seed: u32, indoor: bool) -> Buf {
    let recipe = ShotRecipe {
        body: 105.0,
        tail: 0.09,
        bright: 4000.0,
        echo: 0.40,
    };
    heavy_gunshot_reverb(&recipe, seed, 100.0, 36.0, indoor)
}

#[allow(dead_code)]
pub fn lmg_shot(seed: u32) -> Buf {
    lmg_shot_reverb(seed, false)
}

/// Sniper Rifle: Deafening high-powered explosion with massive sub shockwave and long rolling outdoor thunder tail.
pub fn sniper_shot_reverb(seed: u32, indoor: bool) -> Buf {
    let recipe = ShotRecipe {
        body: 82.0,
        tail: 0.22,
        bright: 3400.0,
        echo: 0.65,
    };
    let shot = heavy_gunshot_reverb(&recipe, seed, 96.0, 26.0, indoor);
    if indoor {
        shot.normalize(0.99)
    } else {
        // Lingering rolling thunder tail (outdoor)
        let mut n = Noise::new(seed.wrapping_add(501));
        let mut lp = Lp::new();
        let roll = render(0.7, |t| {
            lp.run(n.next(), 400.0 * decay(t, 0.2) + 80.0) * env(t, 0.04, 0.35) * 1.3
        });
        shot.mix(&roll, 0.06, 0.85).normalize(0.99)
    }
}

#[allow(dead_code)]
pub fn sniper_shot(seed: u32) -> Buf {
    sniper_shot_reverb(seed, false)
}

/// A gun fitted with a suppressor: a dull "thup" and a mechanical clack.
pub fn suppressed_reverb(body: f32, seed: u32, indoor: bool) -> Buf {
    let mut n = Noise::new(seed);
    let mut lp = Lp::new();
    let mut o = Osc::new();
    let b = render(0.18, |t| {
        let x = n.next();
        lp.run(x, 900.0 * decay(t, 0.03) + 200.0) * decay(t, 0.03) * 1.5
            + o.sine(body * 1.4) * decay(t, 0.025) * 0.5
            + x * decay((t - 0.012).abs(), 0.0015) * 0.25
    });
    let echo_gain = if indoor { 0.25 } else { 0.1 };
    let echo_delay = if indoor { 0.025 } else { 0.06 };
    b.echo(echo_delay, echo_gain, 1).normalize(0.6)
}

#[allow(dead_code)]
pub fn suppressed(body: f32, seed: u32) -> Buf {
    suppressed_reverb(body, seed, false)
}

pub fn laser_reverb(seed: u32, indoor: bool) -> Buf {
    let mut o = Osc::new();
    let mut o2 = Osc::new();
    let mut n = Noise::new(seed);
    let b = render(0.35, |t| {
        let f = 2400.0 * decay(t, 0.06) + 260.0;
        (o.sine(f) + 0.4 * o2.square(f * 0.5 + 30.0 * (t * 60.0).sin())) * env(t, 0.004, 0.09)
            + n.next() * decay(t, 0.01) * 0.3
    });
    if indoor {
        b.echo(0.03, 0.3, 2).normalize(0.8)
    } else {
        b.echo(0.09, 0.2, 1).normalize(0.8)
    }
}

#[allow(dead_code)]
pub fn laser(seed: u32) -> Buf {
    laser_reverb(seed, false)
}

pub fn thunder_reverb(seed: u32, indoor: bool) -> Buf {
    let mut n = Noise::new(seed);
    let mut lp = Lp::new();
    let mut o = Osc::new();
    let mut spark = 0.0f32;
    let base = render(0.9, |t| {
        let x = n.next();
        if n.next() > 0.995 {
            spark = 1.0;
        }
        spark *= 0.993;
        lp.run(x, 1800.0 * decay(t, 0.1) + 120.0) * env(t, 0.003, 0.25) * 1.2
            + x * spark * decay(t, 0.4) * 0.6
            + o.sine(48.0) * decay(t, 0.3) * 0.8
    });
    let thump_decay = if indoor { 0.12 } else { 0.28 };
    let thump = sub_thump(90.0, 35.0, thump_decay, 0.65);
    let out = base.mix(&thump, 0.0, 1.0);
    if indoor {
        out.echo(0.035, 0.35, 2).normalize(0.95)
    } else {
        out.echo(0.12, 0.4, 2).normalize(0.95)
    }
}

#[allow(dead_code)]
pub fn thunder(seed: u32) -> Buf {
    thunder_reverb(seed, false)
}


/// Small mechanical click (bolts, magazines, triggers).
pub fn click(freq: f32, seed: u32, len: f32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp = Bp::new();
    let mut o = Osc::new();
    render(len, |t| {
        bp.run(n.next(), freq, 4.0) * decay(t, 0.004) * 3.0
            + o.sine(freq * 0.5) * decay(t, 0.008) * 0.3
    })
    .normalize(0.7)
}

/// A slide of metal on metal.
pub fn slide(freq: f32, len: f32, seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp = Bp::new();
    render(len, |t| {
        let k = t / len;
        bp.run(n.next(), freq * (0.8 + 0.4 * k), 3.0) * (k * (1.0 - k) * 4.0).min(1.0)
    })
    .normalize(0.45)
}

pub fn mag_out(seed: u32) -> Buf {
    click(2200.0, seed, 0.05)
        .mix(&slide(1800.0, 0.12, seed + 1), 0.02, 0.8)
        .normalize(0.55)
}

pub fn mag_in(seed: u32) -> Buf {
    slide(1500.0, 0.08, seed)
        .mix(&click(2600.0, seed + 1, 0.05), 0.07, 1.0)
        .mix(&click(3200.0, seed + 2, 0.04), 0.1, 0.6)
        .normalize(0.65)
}

pub fn bolt(seed: u32) -> Buf {
    click(2000.0, seed, 0.04)
        .mix(&slide(2400.0, 0.1, seed + 1), 0.03, 0.9)
        .mix(&click(3000.0, seed + 2, 0.05), 0.14, 1.1)
        .normalize(0.7)
}

pub fn shell(seed: u32) -> Buf {
    slide(1200.0, 0.06, seed)
        .mix(&click(1700.0, seed + 1, 0.05), 0.05, 1.0)
        .normalize(0.55)
}

/// Ground surface kind for procedural footstep acoustics.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    /// Crisp slap with high transient snap.
    Concrete,
    /// Low-pass rustle with dull thump.
    Grass,
    /// Resonant metallic ping transient.
    Metal,
    /// Squishy splash transient.
    Puddle,
    /// Soft granular crunch / sinking footstep.
    Sand,
    /// Resonant, hollow timber board clack.
    Wood,
}

/// Dynamic procedural footstep tailored to ground material.
pub fn step_surface(seed: u32, surface: Surface) -> Buf {
    match surface {
        Surface::Concrete => {
            let mut n = Noise::new(seed);
            let mut lp = Lp::new();
            let mut bp = Bp::new();
            let mut hp = Hp::new();
            render(0.15, |t| {
                let x = n.next();
                // Crisp transient high-pass snap in first 4ms
                let snap = hp.run(x, 3200.0) * decay(t, 0.0035) * 2.6;
                // Dense heel strike
                let heel = lp.run(x, 1600.0) * decay(t, 0.022) * 1.7;
                // Crisp stone gritty slap
                let grit = bp.run(x, 4200.0, 1.4) * decay(t, 0.032) * 0.55;
                let toe = if t > 0.035 {
                    lp.run(x, 1800.0) * decay(t - 0.035, 0.018) * 0.9
                } else {
                    0.0
                };
                snap + heel + grit + toe
            })
            .normalize(0.55)
        }
        Surface::Grass => {
            let mut n = Noise::new(seed);
            let mut lp_heel = Lp::new();
            let mut lp_rustle = Lp::new();
            let mut osc_thump = Osc::new();
            render(0.18, |t| {
                let x = n.next();
                // Soft low-passed heel strike
                let heel = lp_heel.run(x, 620.0) * decay(t, 0.052) * 1.5;
                // Low-pass rustling foliage/earth
                let rustle = lp_rustle.run(x, 1100.0) * decay(t, 0.065) * 0.7;
                // Dull low-frequency soil thump
                let thump = osc_thump.sine(80.0 * (1.0 + decay(t, 0.02))) * decay(t, 0.045) * 0.5;
                let toe = if t > 0.045 {
                    lp_heel.run(x, 540.0) * decay(t - 0.045, 0.04) * 0.6
                } else {
                    0.0
                };
                heel + rustle + thump + toe
            })
            .normalize(0.5)
        }
        Surface::Metal => {
            let mut n = Noise::new(seed);
            let mut bp_ping = Bp::new();
            let mut hp = Hp::new();
            let mut lp = Lp::new();
            // Resonant metallic ping partials (grate ring)
            let ping_bell = bell(&[(1680.0, 0.6, 0.065), (2940.0, 0.45, 0.045), (4420.0, 0.25, 0.03)], 0.16);
            let impact = render(0.16, |t| {
                let x = n.next();
                let click_snap = hp.run(x, 3400.0) * decay(t, 0.003) * 2.2;
                let grate_slap = bp_ping.run(x, 2200.0, 3.5) * decay(t, 0.025) * 1.4;
                let clank = lp.run(x, 1400.0) * decay(t, 0.03) * 1.1;
                click_snap + grate_slap + clank
            });
            impact.mix(&ping_bell, 0.001, 0.85).normalize(0.55)
        }
        Surface::Puddle => {
            let mut n = Noise::new(seed);
            let mut bp_splash = Bp::new();
            let mut hp_drop = Hp::new();
            let mut lp_slosh = Lp::new();
            let mut osc_plop = Osc::new();
            render(0.18, |t| {
                let x = n.next();
                // Crisp water droplet splash transient
                let drop_snap = hp_drop.run(x, 3800.0) * decay(t, 0.006) * 1.8;
                // Squishy watery filter sweep
                let k = t / 0.18;
                let f = 1900.0 - 1100.0 * k;
                let splash = bp_splash.run(x, f, 2.2) * decay(t, 0.055) * 1.9;
                // Sloshing water low-end
                let slosh = lp_slosh.run(x, 500.0) * decay(t, 0.07) * 1.0;
                // Subtle bubbly droplet plop
                let plop = osc_plop.sine(280.0 + 160.0 * decay(t, 0.03)) * decay(t, 0.04) * 0.4;
                drop_snap + splash + slosh + plop
            })
            .normalize(0.55)
        }
        Surface::Sand => {
            let mut n = Noise::new(seed);
            let mut bp_crunch = Bp::new();
            let mut lp_sink = Lp::new();
            render(0.18, |t| {
                let x = n.next();
                // Granular sand crunch
                let crunch = bp_crunch.run(x, 2400.0, 1.2) * decay(t, 0.04) * 1.6;
                // Soft low-end sinking
                let sink = lp_sink.run(x, 400.0) * decay(t, 0.06) * 1.2;
                crunch + sink
            })
            .normalize(0.45)
        }
        Surface::Wood => {
            let mut n = Noise::new(seed);
            let mut bp_clack = Bp::new();
            let mut lp_body = Lp::new();
            render(0.16, |t| {
                let x = n.next();
                // Sharp timber clack
                let clack = bp_clack.run(x, 1200.0, 2.5) * decay(t, 0.02) * 2.0;
                // Resonant hollow body
                let body = lp_body.run(x, 300.0) * decay(t, 0.08) * 1.5;
                clack + body
            })
            .normalize(0.6)
        }
    }
}

/// Feet on the ground. `soft` is grass (duller, longer).
pub fn step(seed: u32, soft: bool) -> Buf {
    step_surface(seed, if soft { Surface::Grass } else { Surface::Concrete })
}

/// Near-death tinnitus: high-pitch 4 kHz dual-sine ringing tone with subtle beating that fades over 2.5s.
pub fn tinnitus() -> Buf {
    let mut o1 = Osc::new();
    let mut o2 = Osc::new();
    let len = 2.5;
    render(len, |t| {
        // Dual sines at 4000 Hz and 4004 Hz create subtle organic beating
        let ring = o1.sine(4000.0) * 0.7 + o2.sine(4004.0) * 0.3;
        // Fades smoothly over 2.5s
        let envelope = (1.0 - t / len).max(0.0).powf(1.8);
        ring * envelope * 0.42
    })
    .normalize(0.65)
}


pub fn whoosh(len: f32, lo: f32, hi: f32, seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp = Bp::new();
    render(len, |t| {
        let k = t / len;
        let f = lo + (hi - lo) * (k * std::f32::consts::PI).sin();
        bp.run(n.next(), f, 1.2) * (k * std::f32::consts::PI).sin().powf(1.5)
    })
    .normalize(0.6)
}

pub fn thud(freq: f32, len: f32, seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut lp = Lp::new();
    let mut o = Osc::new();
    render(len, |t| {
        lp.run(n.next(), 600.0) * decay(t, len * 0.2) * 1.2
            + o.sine(freq * (1.0 + decay(t, 0.02))) * decay(t, len * 0.3)
    })
    .normalize(0.8)
}

/// A voice-like sound: a buzzy source through two formant filters.
pub struct Voice {
    pub pitch: f32,
    pub pitch_end: f32,
    pub formants: (f32, f32),
    pub formants_end: (f32, f32),
    pub rasp: f32,
    pub len: f32,
    pub attack: f32,
}

pub fn voice(v: &Voice, seed: u32) -> Buf {
    let mut o = Osc::new();
    let mut n = Noise::new(seed);
    let (mut f1, mut f2, mut lp) = (Bp::new(), Bp::new(), Lp::new());
    let wobble = 3.0 + (seed % 5) as f32;
    render(v.len, |t| {
        let k = t / v.len;
        let pitch =
            (v.pitch + (v.pitch_end - v.pitch) * k) * (1.0 + 0.04 * (t * wobble * TAU).sin());
        let src = o.saw(pitch) * (1.0 - v.rasp) + n.next() * v.rasp;
        let fa = v.formants.0 + (v.formants_end.0 - v.formants.0) * k;
        let fb = v.formants.1 + (v.formants_end.1 - v.formants.1) * k;
        let s = f1.run(src, fa, 5.0) + 0.6 * f2.run(src, fb, 6.0);
        let shape = if k < v.attack {
            k / v.attack
        } else {
            ((1.0 - k) / (1.0 - v.attack)).powf(0.7)
        };
        lp.run(s, 3000.0) * shape
    })
    .normalize(0.8)
}

/// Bright decaying partials (bells, chimes, music-box tines).
pub fn bell(partials: &[(f32, f32, f32)], len: f32) -> Buf {
    let mut oscs: Vec<Osc> = partials.iter().map(|_| Osc::new()).collect();
    render(len, |t| {
        partials
            .iter()
            .zip(oscs.iter_mut())
            .map(|((f, amp, tau), o)| o.sine(*f) * amp * env(t, 0.002, *tau))
            .sum::<f32>()
    })
    .normalize(0.7)
}

/// A sequence of tine notes (frequency, start time).
pub fn melody(notes: &[(f32, f32)], note_len: f32) -> Buf {
    let mut out = Buf(Vec::new());
    for (f, at) in notes {
        let tine = bell(
            &[
                (*f, 1.0, 0.25),
                (f * 2.0, 0.35, 0.12),
                (f * 3.01, 0.15, 0.06),
            ],
            note_len,
        );
        out = out.mix(&tine, *at, 1.0);
    }
    out.normalize(0.6)
}

/// Meaty CoD Zombies hitmarker: low-mid flesh thud/squelch + crisp mechanical 'thwip/tick' transient.
pub fn hitmarker_tick(seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp_tick = Bp::new();
    let mut hp_snap = Hp::new();
    let mut osc_thud = Osc::new();
    let mut lp_squish = Lp::new();

    render(0.09, |t| {
        let x = n.next();
        // Crisp mechanical 'thwip/tick' transient (first 2-4ms)
        let tick = bp_tick.run(x, 4400.0, 4.0) * decay(t, 0.0035) * 2.2
            + hp_snap.run(x, 5600.0) * decay(t, 0.0018) * 1.6;

        // Low-mid fleshy impact thud (180 Hz dropping to 70 Hz)
        let thud_freq = 70.0 + 110.0 * decay(t, 0.015);
        let thud = osc_thud.sine(thud_freq) * decay(t, 0.045) * 1.5;

        // Subtle wet squelch body
        let squelch = lp_squish.run(x, 1400.0 * decay(t, 0.02) + 250.0) * decay(t, 0.035) * 1.1;

        tick * 1.2 + thud + squelch
    })
    .normalize(0.92)
}

/// Visceral skull-pop headshot: sharp bone fracture snap + wet fleshy gore burst + crisp confirmation ring.
pub fn skull_pop(seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp_crack = Bp::new();
    let mut bp_crack2 = Bp::new();
    let mut lp_gore = Lp::new();
    let mut hp = Hp::new();
    let mut osc_gore = Osc::new();
    let mut osc_sub = Osc::new();
    let mut osc_bell1 = Osc::new();
    let mut osc_bell2 = Osc::new();
    let mut osc_bell3 = Osc::new();

    render(0.24, |t| {
        let x = n.next();

        // 1. Sharp bone fracture snap (crunchy high-Q transient crackle)
        let snap1 = bp_crack.run(x, 3200.0, 3.2) * decay(t, 0.004) * 2.4;
        let snap2 = bp_crack2.run(x, 5400.0, 4.0) * decay(t, 0.0025) * 2.0;
        let crackle = hp.run(x, 4000.0) * if n.next() > 0.85 { 1.8 } else { 0.2 } * decay(t, 0.018);
        let bone_break = snap1 + snap2 + crackle;

        // 2. Wet fleshy gore burst & squelch
        let gore_fm = 240.0 + 160.0 * osc_gore.sine(45.0);
        let gore = lp_gore.run(x, gore_fm * (1.0 + 2.0 * decay(t, 0.03))) * decay(t, 0.07) * 2.2;
        let gore_thump = osc_sub.sine(85.0 + 40.0 * decay(t, 0.02)) * decay(t, 0.055) * 1.3;

        // 3. Crisp confirmation chime/ring (iconic reward ding overtone)
        let ring1 = osc_bell1.sine(2793.8) * decay(t, 0.16) * 0.9; // F7-ish
        let ring2 = osc_bell2.sine(4186.0) * decay(t, 0.11) * 0.55; // C8-ish
        let ring3 = osc_bell3.sine(5587.6) * decay(t, 0.06) * 0.35;
        let confirmation = ring1 + ring2 + ring3;

        bone_break * 1.4 + gore * 1.3 + gore_thump + confirmation * 1.1
    })
    .normalize(0.96)
}

/// Heavy, squishy bone-breaking zombie kill finish sound.
pub fn kill_sound(seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp_bone = Bp::new();
    let mut lp_flesh = Lp::new();
    let mut osc_thud = Osc::new();
    let mut osc_drop = Osc::new();

    let crunch = render(0.18, |t| {
        let x = n.next();
        // Heavy skull/bone break snap
        let bone = bp_bone.run(x, 2200.0, 2.5) * decay(t, 0.008) * 2.5;
        // Meaty squish finish
        let squish = lp_flesh.run(x, 1100.0 * decay(t, 0.03) + 200.0) * decay(t, 0.09) * 1.8;
        // Heavy concussive impact thud
        let thud = osc_thud.sine(65.0 + 60.0 * decay(t, 0.025)) * decay(t, 0.08) * 1.4;
        // Satisfying low bass drop
        let sub = osc_drop.sine(42.0) * decay(t, 0.12) * 1.1;

        bone + squish + thud + sub
    });

    let bell_confirmation = bell(&[(1480.0, 0.8, 0.08), (2220.0, 0.4, 0.05)], 0.15);
    crunch.mix(&bell_confirmation, 0.002, 0.5).normalize(0.95)
}

/// Wet fleshy bullet impact squelch (for zombie hurt audio).
pub fn flesh_impact(seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp_wet = Bp::new();
    let mut lp_meat = Lp::new();
    let mut osc_thud = Osc::new();

    render(0.12, |t| {
        let x = n.next();
        let slap = bp_wet.run(x, 1600.0, 1.8) * decay(t, 0.006) * 2.0;
        let squelch = lp_meat.run(x, 800.0 * decay(t, 0.02) + 180.0) * decay(t, 0.06) * 1.5;
        let thud = osc_thud.sine(120.0 * decay(t, 0.015) + 60.0) * decay(t, 0.05) * 1.2;
        slap + squelch + thud
    })
    .normalize(0.85)
}

/// A crisp high-frequency metallic "crit / headshot" ding sound layered with bone snap and gore.
pub fn headshot_ding(seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp_crack = Bp::new();
    let mut hp_snap = Hp::new();
    let mut lp_flesh = Lp::new();
    let mut o_sub = Osc::new();
    let mut o_bell1 = Osc::new();
    let mut o_bell2 = Osc::new();
    let mut o_bell3 = Osc::new();

    render(0.26, |t| {
        let x = n.next();
        // 1. Sharp bone fracture transient
        let crack = bp_crack.run(x, 4800.0, 3.5) * decay(t, 0.003) * 2.5
            + hp_snap.run(x, 6200.0) * decay(t, 0.002) * 2.0;

        // 2. Fleshy gore squelch
        let gore = lp_flesh.run(x, 900.0 * decay(t, 0.02) + 200.0) * decay(t, 0.05) * 1.8;
        let thump = o_sub.sine(75.0 + 35.0 * decay(t, 0.015)) * decay(t, 0.04) * 1.2;

        // 3. Distinct high-frequency metallic reward ding (F#7 / C#8 / F#8 harmonics)
        let bell1 = o_bell1.sine(2960.0) * decay(t, 0.22) * 1.2;
        let bell2 = o_bell2.sine(4440.0) * decay(t, 0.14) * 0.7;
        let bell3 = o_bell3.sine(5920.0) * decay(t, 0.08) * 0.4;
        let ping = bell1 + bell2 + bell3;

        crack * 1.2 + gore + thump + ping * 1.3
    })
    .normalize(0.96)
}

#[allow(dead_code)]
pub fn crit_ding(seed: u32) -> Buf {
    headshot_ding(seed)
}

pub fn explosion(seed: u32, size: f32) -> Buf {
    let mut n = Noise::new(seed);
    let mut lp = Lp::new();
    let mut lp2 = Lp::new();
    let mut o = Osc::new();
    let mut crackle = 0.0f32;
    let len = 1.2 * size;
    let base = render(len, |t| {
        let x = n.next();
        if n.next() > 0.997 {
            crackle = 1.0;
        }
        crackle *= 0.99;
        let boom = lp.run(x, 3500.0 * decay(t, 0.05) + 150.0) * env(t, 0.002, 0.25 * size) * 1.4;
        let sub = o.sine(42.0 + 30.0 * decay(t, 0.05)) * decay(t, 0.35 * size) * 1.2;
        let debris = lp2.run(x, 2500.0) * crackle * decay(t, 0.6 * size) * 0.5;
        boom + sub + debris
    });
    let punch = sub_thump(90.0, 32.0, 0.16 * size, (0.6 * size).min(len));
    base.mix(&punch, 0.0, 1.1).normalize(1.0)
}

/// Boss slam / heavy ground slam: a shockwave blast combined with visceral sub-bass thump and ground rumble.
pub fn boss_slam(seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut lp = Lp::new();
    let mut bp = Bp::new();
    let impact = render(0.7, |t| {
        let x = n.next();
        let crack = bp.run(x, 1100.0, 1.5) * decay(t, 0.015) * 2.2;
        let blast = lp.run(x, 550.0 * decay(t, 0.08) + 110.0) * decay(t, 0.35) * 1.5;
        crack + blast
    });
    let thump = sub_thump(95.0, 32.0, 0.22, 0.65);
    impact
        .mix(&thump, 0.0, 1.3)
        .mix(&rumble(0.65, seed + 1), 0.03, 0.75)
        .normalize(1.0)
}

pub fn slam(seed: u32) -> Buf {
    boss_slam(seed)
}

pub fn zap(seed: u32, len: f32) -> Buf {
    let mut n = Noise::new(seed);
    let mut o = Osc::new();
    let mut bp = Bp::new();
    render(len, |t| {
        let buzz = o.square(110.0 + 40.0 * (t * 37.0).sin());
        let fizz = bp.run(n.next(), 3500.0, 2.0) * if n.next() > 0.6 { 1.0 } else { 0.2 };
        (buzz * 0.35 + fizz) * env(t, 0.005, len * 0.4)
    })
    .normalize(0.7)
}

pub fn fire(seed: u32, len: f32) -> Buf {
    let mut n = Noise::new(seed);
    let mut lp = Lp::new();
    let mut bp = Bp::new();
    render(len, |t| {
        let k = t / len;
        let x = n.next();
        let roar = lp.run(x, 600.0 + 1500.0 * (k * 3.0).min(1.0)) * 1.4;
        let hiss = bp.run(x, 5000.0, 0.8) * 0.3;
        let pop = if n.next() > 0.998 { x * 4.0 } else { 0.0 };
        (roar + hiss + pop) * env(t, len * 0.15, len * 0.35)
    })
    .normalize(0.75)
}

pub fn ice(seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp = Bp::new();
    let crack = render(0.4, |t| {
        bp.run(n.next(), 4500.0, 1.5) * decay(t, 0.03) * 2.0
    });
    bell(
        &[
            (2320.0, 0.6, 0.35),
            (3150.0, 0.5, 0.3),
            (4710.0, 0.4, 0.2),
            (6230.0, 0.25, 0.15),
        ],
        0.9,
    )
    .mix(&crack, 0.0, 1.0)
    .normalize(0.7)
}

pub fn chime_up(base: f32, steps: &[f32], gap: f32) -> Buf {
    let notes: Vec<(f32, f32)> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| (base * s, i as f32 * gap))
        .collect();
    melody(&notes, 0.6)
}

pub fn rumble(len: f32, seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut lp = Lp::new();
    render(len, |t| {
        let rattle = 0.6 + 0.4 * (t * 38.0 * TAU).sin();
        lp.run(n.next(), 400.0) * rattle * env(t, 0.1, len * 0.5) * 2.0
    })
    .normalize(0.7)
}

/// Low-health tension heartbeat: a deep physiological "lub-dub" double thump.
/// First thump (lub): low sine (56 Hz -> 36 Hz) + muffled chest cavity lowpass noise.
/// Second thump (dub): slightly lighter (64 Hz -> 42 Hz) spaced ~0.12s later.
pub fn heartbeat(seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut lp_chest = Lp::new();
    let mut osc1 = Osc::new();
    let mut osc2 = Osc::new();

    let len = 0.45;
    render(len, |t| {
        let x = n.next();
        // Lub pulse (at t = 0.0)
        let t1 = t;
        let lub_pitch = 36.0 + 20.0 * decay(t1, 0.04);
        let lub_sine = osc1.sine(lub_pitch) * decay(t1, 0.08);
        let lub_chest = lp_chest.run(x, 110.0) * decay(t1, 0.06) * 0.8;
        let lub = lub_sine + lub_chest;

        // Dub pulse (at t = 0.12)
        let t2 = t - 0.12;
        let dub = if t2 > 0.0 {
            let dub_pitch = 42.0 + 22.0 * decay(t2, 0.035);
            let dub_sine = osc2.sine(dub_pitch) * decay(t2, 0.065) * 0.75;
            let dub_chest = lp_chest.run(x, 130.0) * decay(t2, 0.05) * 0.6;
            dub_sine + dub_chest
        } else {
            0.0
        };

        lub + dub
    })
    .normalize(0.95)
}

/// Atmospheric eerie wind: slowly sweeping bandpassed noise gust with resonant hollow whistling.
pub fn ambient_wind(len: f32, seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp1 = Bp::new();
    let mut bp2 = Bp::new();
    let mut lp = Lp::new();
    render(len, |t| {
        let k = t / len;
        // Gust envelope (smooth swell and taper)
        let env_shape = (k * std::f32::consts::PI).sin().powf(1.4);
        let x = n.next();
        // Low rumble gust
        let low = lp.run(x, 220.0 + 80.0 * (t * 2.2).sin()) * 1.2;
        // Whistling wind through gaps
        let f_whistle = 450.0 + 220.0 * (t * 1.8 * TAU).sin() + 60.0 * (t * 5.1).sin();
        let whistle = bp1.run(x, f_whistle, 3.5) * 0.8;
        // Air turbulence
        let turbulence = bp2.run(x, 1200.0 + 400.0 * (t * 1.3).sin(), 1.5) * 0.35;
        (low + whistle + turbulence) * env_shape
    })
    .normalize(0.65)
}

/// Atmospheric ambient drone: unsettling low harmonic drone with slow beat frequency.
pub fn ambient_drone(len: f32, seed: u32) -> Buf {
    let mut o1 = Osc::new();
    let mut o2 = Osc::new();
    let mut o3 = Osc::new();
    let mut n = Noise::new(seed);
    let mut bp = Bp::new();
    render(len, |t| {
        let k = t / len;
        let env_shape = (k * std::f32::consts::PI).sin();
        // Detuned low drone creating eerie 1.2 Hz beating
        let sub1 = o1.sine(55.0);
        let sub2 = o2.sine(56.2) * 0.8;
        // Fifth overtone for ominous mood
        let overtone = o3.sine(82.4 + 1.5 * (t * 3.0).sin()) * 0.35;
        // Subtle cold air resonance
        let cold = bp.run(n.next(), 380.0, 4.0) * 0.25;
        (sub1 + sub2 + overtone + cold) * env_shape
    })
    .normalize(0.60)
}

/// Ability ready cue: a bright, crystalline rising chime arpeggio signaling cooldown completion.
pub fn ability_ready(seed: u32) -> Buf {
    let notes = [
        (1046.5, 0.00), // C6
        (1318.5, 0.05), // E6
        (1568.0, 0.10), // G6
        (2093.0, 0.15), // C7
    ];
    let mut out = Buf(Vec::new());
    for (f, at) in notes {
        let chime = bell(
            &[
                (f, 1.0, 0.18),
                (f * 2.0, 0.45, 0.10),
                (f * 3.01, 0.2, 0.05),
            ],
            0.28,
        );
        out = out.mix(&chime, at, 0.85);
    }
    // Subtle low-mid energy swell
    let mut o = Osc::new();
    let mut n = Noise::new(seed);
    let mut bp = Bp::new();
    let sweep = render(0.24, |t| {
        let f = 350.0 + 600.0 * (t / 0.24);
        o.sine(f) * env(t, 0.02, 0.12) * 0.4 + bp.run(n.next(), f * 1.5, 2.5) * decay(t, 0.08) * 0.2
    });
    out.mix(&sweep, 0.0, 0.6).normalize(0.85)
}

/// Round start stinger: dramatic apocalyptic brass/horn swell + heavy sub concussion + dark bells.
pub fn round_start_stinger(seed: u32) -> Buf {
    let mut o_saw = Osc::new();
    let mut lp_brass = Lp::new();
    let mut n = Noise::new(seed);
    let mut bp_air = Bp::new();

    // 1. Apocalyptic brass swell
    let brass = render(1.8, |t| {
        let cutoff = 120.0 + 650.0 * env(t, 0.35, 0.7);
        let tone = o_saw.saw(73.4) * 0.8; // D2
        let air = bp_air.run(n.next(), cutoff * 1.4, 2.0) * 0.3;
        lp_brass.run(tone + air, cutoff) * env(t, 0.25, 0.9) * 1.6
    });

    // 2. Heavy sub concussion impact
    let sub = sub_thump(90.0, 32.0, 0.25, 0.7);

    // 3. Dark ominous church bell / gong partials
    let bells = bell(
        &[
            (110.0, 1.0, 2.2),
            (165.0, 0.7, 1.8),
            (220.0, 0.5, 1.4),
            (330.0, 0.3, 0.9),
        ],
        2.5,
    );

    bells
        .mix(&brass, 0.05, 0.9)
        .mix(&sub, 0.0, 1.1)
        .normalize(0.95)
}

/// Countdown tone (low pips for 3, 2, 1, high pip for 0 / round start).
pub fn countdown_beep(high: bool, seed: u32) -> Buf {
    let mut o = Osc::new();
    let mut o2 = Osc::new();
    let mut n = Noise::new(seed);
    let mut bp = Bp::new();

    let (freq, len, tau) = if high {
        (1760.0, 0.16, 0.09) // A6
    } else {
        (880.0, 0.10, 0.05)  // A5
    };

    render(len, |t| {
        let tone = o.sine(freq) + o2.sine(freq * 2.0) * 0.25;
        let click = bp.run(n.next(), freq * 1.5, 4.0) * decay(t, 0.002) * 0.4;
        (tone + click) * env(t, 0.003, tau)
    })
    .normalize(0.8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(b: &Buf) {
        assert!(!b.0.is_empty());
        assert!(b.0.iter().all(|s| s.is_finite()));
        let peak = b.0.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.1 && peak <= 1.0, "peak {peak}");
    }

    #[test]
    fn sounds_are_sane() {
        ok(&gunshot(
            &ShotRecipe {
                body: 140.0,
                tail: 0.07,
                bright: 4500.0,
                echo: 0.35,
            },
            1,
        ));
        ok(&suppressed(160.0, 2));
        ok(&laser(3));
        ok(&thunder(4));
        ok(&mag_in(5));
        ok(&step(6, true));
        ok(&voice(
            &Voice {
                pitch: 80.0,
                pitch_end: 60.0,
                formants: (500.0, 900.0),
                formants_end: (600.0, 1000.0),
                rasp: 0.3,
                len: 1.0,
                attack: 0.2,
            },
            7,
        ));
        ok(&explosion(8, 1.0));
        ok(&ice(9));
        ok(&melody(&[(659.0, 0.0), (784.0, 0.2)], 0.5));
        ok(&rumble(1.0, 10));
        ok(&zap(11, 0.4));
        ok(&fire(12, 1.0));
        ok(&sub_thump(90.0, 35.0, 0.1, 0.3));
        ok(&crit_ding(13));
        ok(&headshot_ding(14));
        ok(&boss_slam(15));
        ok(&slam(16));
        ok(&heavy_gunshot(
            &ShotRecipe {
                body: 90.0,
                tail: 0.16,
                bright: 3000.0,
                echo: 0.55,
            },
            17,
            90.0,
            35.0,
        ));
        ok(&pistol_shot(18));
        ok(&magnum_shot(19));
        ok(&smg_shot(20));
        ok(&rifle_shot(21));
        ok(&shotgun_shot(22));
        ok(&lmg_shot(23));
        ok(&sniper_shot(24));
        ok(&hitmarker_tick(25));
        ok(&skull_pop(26));
        ok(&kill_sound(27));
        ok(&flesh_impact(28));
        ok(&heartbeat(29));
        ok(&ambient_wind(2.0, 30));
        ok(&ambient_drone(2.0, 31));
        ok(&ability_ready(32));
        ok(&round_start_stinger(33));
        ok(&step_surface(6, Surface::Concrete));
        ok(&step_surface(7, Surface::Grass));
        ok(&step_surface(8, Surface::Metal));
        ok(&step_surface(9, Surface::Puddle));
        ok(&tinnitus());
        ok(&countdown_beep(false, 34));
        ok(&countdown_beep(true, 35));
        let w = gunshot(
            &ShotRecipe {
                body: 140.0,
                tail: 0.07,
                bright: 4500.0,
                echo: 0.35,
            },
            1,
        )
        .wav();
        if let Ok(dir) = std::env::var("FPS_WAV_DIR") {
            std::fs::write(format!("{dir}/rifle.wav"), &w).unwrap();
            std::fs::write(
                format!("{dir}/groan.wav"),
                voice(
                    &Voice {
                        pitch: 80.0,
                        pitch_end: 60.0,
                        formants: (500.0, 900.0),
                        formants_end: (600.0, 1000.0),
                        rasp: 0.3,
                        len: 1.0,
                        attack: 0.2,
                    },
                    7,
                )
                .wav(),
            )
            .unwrap();
        }
        assert_eq!(&w[0..4], b"RIFF");
    }

    #[test]
    fn test_pitch_from_cents_conversion() {
        assert_eq!(pitch_from_cents(0.0), 1.0);
        assert!((pitch_from_cents(1200.0) - 2.0).abs() < 1e-6);
        assert!((pitch_from_cents(-1200.0) - 0.5).abs() < 1e-6);
        assert!((pitch_from_cents(2400.0) - 4.0).abs() < 1e-6);
        assert!((pitch_from_cents(-2400.0) - 0.25).abs() < 1e-6);
        let semitone = pitch_from_cents(100.0);
        assert!((semitone - 2.0_f32.powf(1.0 / 12.0)).abs() < 1e-6);
    }

    #[test]
    fn test_gain_from_db_conversion() {
        assert_eq!(gain_from_db(0.0), 1.0);
        assert!((gain_from_db(20.0) - 10.0).abs() < 1e-5);
        assert!((gain_from_db(-20.0) - 0.1).abs() < 1e-5);
        assert!((gain_from_db(40.0) - 100.0).abs() < 1e-4);
        assert!((gain_from_db(-6.0205999) - 0.5).abs() < 1e-5);
        assert!((gain_from_db(6.0205999) - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_poll_window_normal_window() {
        use super::super::poll_window;

        let events = [(0.0, 0), (5.0, 1), (15.0, 2)];
        // Before first event (0.3 to 4.9): nothing fires
        assert_eq!(poll_window(&events, Some(0.3), 4.9, false), Vec::<i32>::new());
        // Exact landing on 5.0: event 1 fires
        assert_eq!(poll_window(&events, Some(4.9), 5.0, false), vec![1]);
        // Immediately after 5.0: event 1 does not fire again (half-open (last, current])
        assert_eq!(poll_window(&events, Some(5.0), 5.5, false), Vec::<i32>::new());
        // Progression across 15.0: event 2 fires
        assert_eq!(poll_window(&events, Some(14.0), 15.0, false), vec![2]);
    }

    #[test]
    fn test_poll_window_jump_over_window() {
        use super::super::poll_window;

        // Frame spike: large dt jumps past multiple events in a single tick
        let events = [(2.0, "step"), (5.0, "swing"), (8.0, "impact"), (12.0, "recover")];
        // Jump from frame 1.0 to frame 10.0: should catch events at 2.0, 5.0, and 8.0 without missing
        let fired = poll_window(&events, Some(1.0), 10.0, false);
        assert_eq!(fired, vec!["step", "swing", "impact"]);
        // Next frame moves from 10.0 to 13.0: should catch event at 12.0
        let fired_next = poll_window(&events, Some(10.0), 13.0, false);
        assert_eq!(fired_next, vec!["recover"]);
    }

    #[test]
    fn test_poll_window_loop_wrap_around() {
        use super::super::{heard_frame, poll_window};

        let events = [(0.0, 0), (5.0, 1), (15.0, 2)];
        // A loop wrapping from 14.0 to 2.0 passes 15.0 (tail of loop), then 0.0 (head of next cycle)
        assert_eq!(poll_window(&events, Some(14.0), 2.0, true), vec![0, 2]);

        // Wall-clock continuous loop with rem_euclid (e.g. 16-frame loop from 0 to 15)
        let loop_events = [(10.0, 42)];
        let clip_frames = 16;
        let mut fired_count = 0;
        let mut last = None;
        let mut clock = 0.0_f32;
        // Run 5 full loop iterations with arbitrary frame step
        while clock < 80.0 {
            let frame = heard_frame(clock, true, clip_frames);
            let triggers = poll_window(&loop_events, last, frame, true);
            fired_count += triggers.len();
            last = Some(frame);
            clock += 0.73; // non-integer step to ensure landing between frames
        }
        assert_eq!(fired_count, 5);

        // Action restarting from beginning (looped = false)
        assert_eq!(poll_window(&events, Some(20.0), 0.5, false), vec![0]);

        // Initial frame start (last == None)
        assert_eq!(poll_window(&events, None, 0.3, false), vec![0]);
    }

    #[test]
    fn test_poll_window_empty_cases() {
        use super::super::poll_window;

        // 1. Empty events list
        let empty_events: &[(f32, usize)] = &[];
        assert_eq!(poll_window(empty_events, Some(1.0), 10.0, false), Vec::<usize>::new());
        assert_eq!(poll_window(empty_events, None, 1.0, false), Vec::<usize>::new());
        assert_eq!(poll_window(empty_events, Some(10.0), 2.0, true), Vec::<usize>::new());

        // 2. No events falling in the window
        let events = [(1.0, 10), (10.0, 20)];
        assert_eq!(poll_window(&events, Some(3.0), 7.0, false), Vec::<i32>::new());

        // 3. Same frame (zero time delta)
        assert_eq!(poll_window(&events, Some(1.0), 1.0, false), Vec::<i32>::new());
        assert_eq!(poll_window(&events, Some(10.0), 10.0, true), Vec::<i32>::new());

        // 4. Event exactly at last (exclusive lower bound)
        assert_eq!(poll_window(&events, Some(1.0), 5.0, false), Vec::<i32>::new());

        // 5. Initial call (last == None) with no events in initial window
        assert_eq!(poll_window(&events, None, 5.0, false), Vec::<i32>::new());
    }
}

/// Rattling links: a run of small metal clinks.
pub fn chain(len: f32, seed: u32) -> Buf {
    let mut out = render(len + 0.1, |_| 0.0);
    let mut n = Noise::new(seed);
    let mut t = 0.0;
    let mut i = 0;
    while t < len {
        let f = 2400.0 + 900.0 * n.next();
        let clink = bell(&[(f, 1.0, 0.03), (f * 1.47, 0.5, 0.02)], 0.08)
            .mix(&click(f * 0.8, seed + i, 0.03), 0.0, 0.5);
        out = out.mix(&clink, t, 0.6 * (1.0 - t / len * 0.5));
        t += 0.025 + 0.02 * n.next().abs();
        i += 1;
    }
    out.normalize(0.7)
}

/// A plucked string (bowstrings).
pub fn twang(freq: f32, seed: u32) -> Buf {
    let mut o = Osc::new();
    let mut o2 = Osc::new();
    let mut n = Noise::new(seed);
    let mut lp = Lp::new();
    render(0.45, |t| {
        let f = freq * (1.0 + 0.3 * decay(t, 0.01));
        let pluck = lp.run(n.next(), 3000.0) * decay(t, 0.006);
        (o.saw(f) * 0.6 + o2.sine(f * 2.01) * 0.3) * decay(t, 0.09) + pluck
    })
    .normalize(0.7)
}

/// Steel jaws slamming shut.
pub fn snap(seed: u32) -> Buf {
    click(1800.0, seed, 0.06)
        .mix(&thud(160.0, 0.18, seed + 1), 0.0, 0.8)
        .mix(&bell(&[(1250.0, 0.5, 0.15), (1930.0, 0.35, 0.1)], 0.35), 0.01, 0.6)
        .normalize(0.85)
}

/// Glass or stone breaking into pieces.
pub fn shatter(seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp = Bp::new();
    let mut tinkle = render(0.7, |t| {
        let x = n.next();
        let spark = if n.next() > 0.995 { 3.0 } else { 0.0 };
        bp.run(x, 5200.0, 1.2) * (decay(t, 0.05) * 2.0 + spark * decay(t, 0.25))
    });
    tinkle = tinkle.mix(&thud(220.0, 0.15, seed + 1), 0.0, 0.5);
    tinkle.normalize(0.75)
}

/// A long hiss of gas or spray.
pub fn hiss(len: f32, seed: u32) -> Buf {
    let mut n = Noise::new(seed);
    let mut bp = Bp::new();
    render(len, |t| {
        let k = t / len;
        bp.run(n.next(), 4500.0 - 1500.0 * k, 0.9) * env(t, 0.05, len * 0.4)
    })
    .normalize(0.6)
}

/// A hollow ghostly moan.
pub fn wail(len: f32, seed: u32) -> Buf {
    let mut o = Osc::new();
    let mut o2 = Osc::new();
    let mut n = Noise::new(seed);
    let mut bp = Bp::new();
    render(len, |t| {
        let k = t / len;
        let f = 330.0 + 160.0 * (k * std::f32::consts::PI).sin() + 12.0 * (t * 5.0 * TAU).sin();
        let tone = o.sine(f) * 0.6 + o2.sine(f * 1.5) * 0.25;
        let air = bp.run(n.next(), f * 2.0, 3.0) * 0.5;
        (tone + air) * (k * std::f32::consts::PI).sin()
    })
    .normalize(0.6)
}

impl Buf {
    pub fn from_wav_pcm16(bytes: &[u8]) -> Self {
        let mut f32_samples = Vec::new();
        if bytes.len() > 44 {
            let data = &bytes[44..];
            for chunk in data.chunks_exact(2) {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                f32_samples.push(sample as f32 / 32768.0);
            }
        }
        Buf(f32_samples)
    }
}
