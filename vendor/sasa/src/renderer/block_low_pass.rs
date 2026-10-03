use crate::Frame;

const FILTERED_HZ: f64 = 1500.;
const OPEN_HZ: f64 = 22000.;
const SWEEP_SECONDS: f64 = 0.1;

/// Music-only approximation of the Unity AudioLowPassFilter used by noise fields.
/// Cutoff interpolation follows output time, independent of chart speed or FPS.
pub(super) struct BlockLowPass {
    active: bool,
    bypass: bool,
    cutoff: f64,
    start: f64,
    elapsed: f64,
    sample_rate: u32,
    coefficients: [f64; 5],
    // Independent transposed direct-form II delay state for left and right.
    delay: [[f64; 2]; 2],
}

impl Default for BlockLowPass {
    fn default() -> Self {
        Self {
            active: false,
            bypass: true,
            // Unity initially disables the filter with its cutoff still at 1500.
            cutoff: FILTERED_HZ,
            start: FILTERED_HZ,
            elapsed: SWEEP_SECONDS,
            sample_rate: 0,
            coefficients: [0.; 5],
            delay: [[0.; 2]; 2],
        }
    }
}

impl BlockLowPass {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        self.start = self.cutoff;
        self.elapsed = 0.;
        self.bypass = false;
    }

    pub fn process(&mut self, frame: Frame, sample_rate: u32) -> Frame {
        if self.bypass || sample_rate == 0 {
            return frame;
        }
        let target = if self.active { FILTERED_HZ } else { OPEN_HZ };
        let previous = self.cutoff;
        self.elapsed = (self.elapsed + 1. / sample_rate as f64).min(SWEEP_SECONDS);
        self.cutoff = self.start + (target - self.start) * (self.elapsed / SWEEP_SECONDS);
        if !self.active && self.elapsed >= SWEEP_SECONDS {
            self.bypass = true;
            self.delay = [[0.; 2]; 2];
            return frame;
        }
        if previous != self.cutoff || sample_rate != self.sample_rate {
            self.sample_rate = sample_rate;
            // Keep poles below Nyquist even on low sample-rate devices.
            let omega = std::f64::consts::TAU * self.cutoff.min(sample_rate as f64 * 0.49) / sample_rate as f64;
            let (sin, cos) = omega.sin_cos();
            let alpha = sin / 2.; // Q = 1, Unity component default.
            let a0 = 1. + alpha;
            self.coefficients = [
                (1. - cos) / (2. * a0),
                (1. - cos) / a0,
                (1. - cos) / (2. * a0),
                -2. * cos / a0,
                (1. - alpha) / a0,
            ];
        }
        let [b0, b1, b2, a1, a2] = self.coefficients;
        let mut out = [frame.0, frame.1];
        for (sample, delay) in out.iter_mut().zip(&mut self.delay) {
            let input = *sample as f64;
            let output = b0 * input + delay[0];
            delay[0] = b1 * input - a1 * output + delay[1];
            delay[1] = b2 * input - a2 * output;
            *sample = output as f32;
        }
        Frame(out[0], out[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversal_continues_from_current_cutoff_and_release_bypasses() {
        let mut filter = BlockLowPass::default();
        filter.set_active(true);
        filter.process(Frame(0., 0.), 48000);
        filter.set_active(false);
        for _ in 0..2400 {
            filter.process(Frame(0., 0.), 48000);
        }
        assert!((filter.cutoff - 11750.).abs() < 0.001);
        filter.set_active(true);
        filter.process(Frame(0., 0.), 48000);
        assert!((11740. ..11750.).contains(&filter.cutoff));
        for _ in 0..4800 {
            filter.process(Frame(0., 0.), 48000);
        }
        assert_eq!(filter.cutoff, FILTERED_HZ);
        filter.set_active(false);
        for _ in 0..4801 {
            filter.process(Frame(0., 0.), 48000);
        }
        assert!(filter.bypass);
        let output = filter.process(Frame(0.23, -0.19), 48000);
        assert_eq!((output.0, output.1), (0.23, -0.19));
    }

    #[test]
    fn rapid_switches_and_sample_rate_changes_stay_finite() {
        let mut filter = BlockLowPass::default();
        for i in 0..20000 {
            if i % 137 == 0 {
                filter.set_active(!filter.active);
            }
            let rate = [8000, 22050, 44100, 48000, 96000][i / 4000];
            let output = filter.process(Frame(0.1, -0.1), rate);
            assert!(output.0.is_finite() && output.0.abs() < 1.);
            assert_eq!(output.0, -output.1);
        }
    }
}

