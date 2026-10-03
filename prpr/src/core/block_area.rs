//! Phigros 4.0.1 noise fields. Geometry is shared by input and rendering.
//! See docs/block-area-port.md for the APK methods used to verify semantics.
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct BlockPoint {
    pub x: f32,
    pub y: f32,
}

impl BlockPoint {
    fn world(self, aspect: f32) -> Self {
        Self {
            x: (self.x - 0.5) * 2.,
            y: (self.y - 0.5) * 2. / aspect,
        }
    }

    fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockMove {
    pub time: f64,
    pub end_position: BlockPoint,
    pub ease_type_x: u8,
    pub ease_type_y: u8,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockScale {
    pub time: f64,
    pub anchor: BlockPoint,
    pub scale: BlockPoint,
    pub ease_type_x: u8,
    pub ease_type_y: u8,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockRotate {
    pub time: f64,
    pub anchor: BlockPoint,
    pub rotation: f32,
    pub ease_type: u8,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockArea {
    pub top_right_percentage: BlockPoint,
    pub bottom_left_percentage: BlockPoint,
    pub appear_time: f64,
    pub enable_time: f64,
    pub disable_time: f64,
    pub disappear_time: f64,
    #[serde(default)]
    pub is_subtract: bool,
    #[serde(default)]
    pub move_events: Vec<BlockMove>,
    #[serde(default)]
    pub scale_events: Vec<BlockScale>,
    #[serde(default)]
    pub rotate_events: Vec<BlockRotate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockPhase {
    Hidden,
    Disabled,
    Ready,
    Active,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockGeometry {
    pub center: BlockPoint,
    pub size: BlockPoint,
    pub rotation: f32,
}

impl BlockGeometry {
    pub fn corner(self, x: f32, y: f32) -> BlockPoint {
        let (s, c) = self.rotation.to_radians().sin_cos();
        BlockPoint {
            x: self.center.x + x * self.size.x * c - y * self.size.y * s,
            y: self.center.y + x * self.size.x * s + y * self.size.y * c,
        }
    }

    pub fn contains(self, point: BlockPoint, inset: f32, subtract: bool) -> bool {
        if self.size.x.abs() < 0.0001 || self.size.y.abs() < 0.0001 {
            return false;
        }
        let (s, c) = self.rotation.to_radians().sin_cos();
        let dx = point.x - self.center.x;
        let dy = point.y - self.center.y;
        let x = (dx * c + dy * s) / self.size.x;
        let y = (-dx * s + dy * c) / self.size.y;
        // Normal blocks shrink; subtract blocks expand. Unity caps the inset
        // at one quarter of the local sprite size on each axis.
        let sign = if subtract { 1. } else { -1. };
        let hx = 0.5 + sign * (inset / self.size.x.abs()).min(0.25);
        let hy = 0.5 + sign * (inset / self.size.y.abs()).min(0.25);
        x.abs() <= hx && y.abs() <= hy
    }
}

/// APK GetEase uses 101 samples and linear interpolation, not analytic easing.
pub fn block_ease(progress: f32, kind: u8) -> f32 {
    fn sample(p: f32, kind: u8) -> f32 {
        match kind {
            0 => p,
            13 => 0.,
            14 => 1.,
            1..=12 => {
                let power = i32::from((kind - 1) / 3 + 2);
                match (kind - 1) % 3 {
                    0 => p.powi(power),
                    1 => 1. - (1. - p).powi(power),
                    _ if p < 0.5 => 0.5 * (2. * p).powi(power),
                    _ => 1. - 0.5 * (2. - 2. * p).powi(power),
                }
            }
            _ => p,
        }
    }
    let p = progress.clamp(0., 1.) * 100.;
    let i = p.floor();
    let a = sample(i / 100., kind);
    let b = sample((i + 1.).min(100.) / 100., kind);
    a + (b - a) * (p - i)
}

fn lerp(a: f32, b: f32, t: f64, start: f64, end: f64, ease: u8) -> f32 {
    let p = if end <= start { 1. } else { ((t - start) / (end - start)) as f32 };
    a + (b - a) * block_ease(p, ease)
}

fn rotate(point: BlockPoint, anchor: BlockPoint, degrees: f32) -> BlockPoint {
    let (s, c) = degrees.to_radians().sin_cos();
    let x = point.x - anchor.x;
    let y = point.y - anchor.y;
    BlockPoint {
        x: anchor.x + x * c - y * s,
        y: anchor.y + x * s + y * c,
    }
}

fn safe_div(a: f32, b: f32) -> f32 {
    if b.abs() < f32::from_bits(8) {
        1.
    } else {
        a / b
    }
}

impl BlockArea {
    pub fn validate(&mut self) -> Result<(), &'static str> {
        if !self.top_right_percentage.finite()
            || !self.bottom_left_percentage.finite()
            || ![self.appear_time, self.enable_time, self.disable_time, self.disappear_time]
                .into_iter()
                .all(f64::is_finite)
            || self.appear_time > self.disappear_time
            || self.enable_time > self.disable_time
        {
            return Err("invalid blockArea geometry or lifetime");
        }
        if self
            .move_events
            .iter()
            .any(|e| !e.time.is_finite() || !e.end_position.finite() || e.ease_type_x > 14 || e.ease_type_y > 14)
            || self
                .scale_events
                .iter()
                .any(|e| !e.time.is_finite() || !e.anchor.finite() || !e.scale.finite() || e.ease_type_x > 14 || e.ease_type_y > 14)
            || self
                .rotate_events
                .iter()
                .any(|e| !e.time.is_finite() || !e.anchor.finite() || !e.rotation.is_finite() || e.ease_type > 14)
        {
            return Err("invalid blockArea animation");
        }
        self.move_events.sort_by(|a, b| a.time.total_cmp(&b.time));
        self.scale_events.sort_by(|a, b| a.time.total_cmp(&b.time));
        self.rotate_events.sort_by(|a, b| a.time.total_cmp(&b.time));
        Ok(())
    }

    pub fn phase(&self, t: f64) -> BlockPhase {
        if t < self.appear_time || t >= self.disappear_time {
            BlockPhase::Hidden
        } else if t >= self.enable_time && t < self.disable_time {
            BlockPhase::Active
        } else if t < self.enable_time && t >= self.enable_time - 0.5 {
            BlockPhase::Ready
        } else {
            BlockPhase::Disabled
        }
    }

    pub fn opacity(&self, t: f64) -> f32 {
        if self.phase(t) == BlockPhase::Hidden {
            0.
        } else if self.phase(t) == BlockPhase::Active || self.appear_time >= self.enable_time {
            1.
        } else {
            ((t - self.appear_time) / 0.5).clamp(0., 1.) as f32
        }
    }

    pub fn geometry(&self, t: f64, aspect: f32) -> BlockGeometry {
        let a = self.top_right_percentage.world(aspect);
        let b = self.bottom_left_percentage.world(aspect);
        let origin = BlockPoint {
            x: (a.x + b.x) * 0.5,
            y: (a.y + b.y) * 0.5,
        };
        let mut center = origin;
        let mut size = BlockPoint {
            x: (a.x - b.x).abs(),
            y: (a.y - b.y).abs(),
        };
        // APK semantics: the key at the START of a segment supplies its ease
        // and anchor. Replay completed anchor deltas so seeking is stateless.
        let count = self.scale_events.partition_point(|e| e.time <= t);
        if count > 0 {
            let index = count - 1;
            let mut scale = self.scale_events[index].scale;
            for i in 0..=index {
                let e = &self.scale_events[i];
                if let Some(next) = self.scale_events.get(i + 1) {
                    let value = if i < index {
                        next.scale
                    } else {
                        BlockPoint {
                            x: lerp(e.scale.x, next.scale.x, t, e.time, next.time, e.ease_type_x),
                            y: lerp(e.scale.y, next.scale.y, t, e.time, next.time, e.ease_type_y),
                        }
                    };
                    let anchor = e.anchor.world(aspect);
                    center.x = anchor.x + (center.x - anchor.x) * safe_div(value.x, e.scale.x);
                    center.y = anchor.y + (center.y - anchor.y) * safe_div(value.y, e.scale.y);
                    if i == index {
                        scale = value;
                    }
                }
            }
            size.x *= scale.x;
            size.y *= scale.y;
        }
        let mut rotation = 0.;
        let count = self.rotate_events.partition_point(|e| e.time <= t);
        if count > 0 {
            let index = count - 1;
            rotation = self.rotate_events[index].rotation;
            for i in 0..=index {
                let e = &self.rotate_events[i];
                if let Some(next) = self.rotate_events.get(i + 1) {
                    let value = if i < index {
                        next.rotation
                    } else {
                        lerp(e.rotation, next.rotation, t, e.time, next.time, e.ease_type)
                    };
                    center = rotate(center, e.anchor.world(aspect), value - e.rotation);
                    if i == index {
                        rotation = value;
                    }
                }
            }
        }
        let count = self.move_events.partition_point(|e| e.time <= t);
        if count > 0 {
            let e = &self.move_events[count - 1];
            let position = if let Some(next) = self.move_events.get(count) {
                BlockPoint {
                    x: lerp(e.end_position.x, next.end_position.x, t, e.time, next.time, e.ease_type_x),
                    y: lerp(e.end_position.y, next.end_position.y, t, e.time, next.time, e.ease_type_y),
                }
            } else {
                e.end_position
            };
            let position = position.world(aspect);
            center.x += position.x - origin.x;
            center.y += position.y - origin.y;
        }
        size.x = size.x.abs();
        size.y = size.y.abs();
        BlockGeometry { center, size, rotation }
    }
}

/// Both the original and inset masks must be solid. Subtract masks toggle
/// by parity, including subtract-only regions (they are not ordinary holes).
pub fn blocks_touch(areas: &[BlockArea], t: f64, aspect: f32, point: BlockPoint) -> bool {
    let mut normal = false;
    let mut subtract = false;
    let mut inset_normal = false;
    let mut inset_subtract = false;
    for area in areas {
        if area.phase(t) != BlockPhase::Active {
            continue;
        }
        let geometry = area.geometry(t, aspect);
        if geometry.contains(point, 0., area.is_subtract) {
            if area.is_subtract {
                subtract = !subtract;
            } else {
                normal = true;
            }
        }
        if geometry.contains(point, 0.03 * 2. / aspect, area.is_subtract) {
            if area.is_subtract {
                inset_subtract = !inset_subtract;
            } else {
                inset_normal = true;
            }
        }
    }
    (normal ^ subtract) && (inset_normal ^ inset_subtract)
}

#[derive(Default)]
pub struct BlockedFingers(HashSet<u64>);

impl BlockedFingers {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn retain(&mut self, alive: impl Fn(&u64) -> bool) {
        self.0.retain(alive);
    }

    /// A blocked contact remains blocked until release, even if the field
    /// disappears or the finger slides outside it. A new contact may reuse ID.
    pub fn filter(&mut self, id: u64, started: bool, ended: bool, inside: bool) -> bool {
        if started {
            self.0.remove(&id);
        }
        if ended {
            self.0.remove(&id);
            return false;
        }
        if inside {
            self.0.insert(id);
        }
        self.0.contains(&id)
    }
}
