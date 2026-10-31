//! Procedural UI icons, drawn once at startup from signed-distance shapes.
//! All icons are white on transparent; UI nodes tint them.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Galaxy,
    Star,
    Planet,
    Colony,
    Rocket,
    Flask,
    Box,
    Atom,
    Gear,
    Pick,
    Flame,
    Factory,
    Bolt,
    House,
    Shield,
    Pause,
    Play,
    Fast,
    Person,
    Warning,
    Cube,
    Gem,
    Belt,
    Arm,
    Trash,
    Close,
    Chevrons,
    Clock,
    Infinity,
    Swirl,
    Target,
    Cursor,
    Rotate,
    Disk,
    Plus,
    Minus,
    Eye,
    Book,
}

impl Icon {
    pub const ALL: [Icon; 38] = [
        Icon::Galaxy,
        Icon::Star,
        Icon::Planet,
        Icon::Colony,
        Icon::Rocket,
        Icon::Flask,
        Icon::Box,
        Icon::Atom,
        Icon::Gear,
        Icon::Pick,
        Icon::Flame,
        Icon::Factory,
        Icon::Bolt,
        Icon::House,
        Icon::Shield,
        Icon::Pause,
        Icon::Play,
        Icon::Fast,
        Icon::Person,
        Icon::Warning,
        Icon::Cube,
        Icon::Gem,
        Icon::Belt,
        Icon::Arm,
        Icon::Trash,
        Icon::Close,
        Icon::Chevrons,
        Icon::Clock,
        Icon::Infinity,
        Icon::Swirl,
        Icon::Target,
        Icon::Cursor,
        Icon::Rotate,
        Icon::Disk,
        Icon::Plus,
        Icon::Minus,
        Icon::Eye,
        Icon::Book,
    ];
}

#[derive(Resource)]
pub struct Icons(pub HashMap<Icon, Handle<Image>>);

impl Icons {
    pub fn get(&self, i: Icon) -> Handle<Image> {
        self.0.get(&i).cloned().unwrap_or_default()
    }
}

pub struct IconPlugin;

impl Plugin for IconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, make_icons);
    }
}

fn make_icons(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let map = Icon::ALL
        .iter()
        .map(|i| (*i, images.add(render(*i, 64))))
        .collect();
    commands.insert_resource(Icons(map));
}

// --- signed distance helpers (negative inside), coordinates in [-1, 1], y up ---

fn circle(p: Vec2, c: Vec2, r: f32) -> f32 {
    p.distance(c) - r
}

fn ring(p: Vec2, c: Vec2, r: f32, w: f32) -> f32 {
    (p.distance(c) - r).abs() - w
}

fn rect(p: Vec2, c: Vec2, half: Vec2) -> f32 {
    let d = (p - c).abs() - half;
    d.max(Vec2::ZERO).length() + d.x.max(d.y).min(0.0)
}

fn rrect(p: Vec2, c: Vec2, half: Vec2, r: f32) -> f32 {
    rect(p, c, half - Vec2::splat(r)) - r
}

fn seg(p: Vec2, a: Vec2, b: Vec2, w: f32) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = (pa.dot(ba) / ba.dot(ba)).clamp(0.0, 1.0);
    (pa - ba * h).length() - w
}

fn poly(p: Vec2, pts: &[Vec2]) -> f32 {
    // Convex polygon (counter-clockwise).
    let mut d = f32::MIN;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let e = b - a;
        let n = Vec2::new(e.y, -e.x).normalize();
        d = d.max((p - a).dot(n));
    }
    d
}

fn union(a: f32, b: f32) -> f32 {
    a.min(b)
}

fn sub(a: f32, b: f32) -> f32 {
    a.max(-b)
}

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

fn rot(p: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    Vec2::new(c * p.x + s * p.y, -s * p.x + c * p.y)
}

fn chevron(p: Vec2, x: f32, w: f32) -> f32 {
    union(
        seg(p, v(x - 0.18, 0.38), v(x + 0.18, 0.0), w),
        seg(p, v(x - 0.18, -0.38), v(x + 0.18, 0.0), w),
    )
}

fn shape(icon: Icon, p: Vec2) -> f32 {
    let w = 0.075;
    match icon {
        Icon::Galaxy | Icon::Swirl => {
            let r = p.length();
            let a = p.y.atan2(p.x);
            let arms = 2.0;
            let spiral = ((a * arms - r * 7.0).rem_euclid(TAU) - PI).abs() / PI;
            let arm = (spiral - 0.42) * r.max(0.25) * 1.2;
            let d = union(arm.max(r - 0.92), circle(p, Vec2::ZERO, 0.18));
            if icon == Icon::Swirl {
                d.max(r - 0.8)
            } else {
                d
            }
        }
        Icon::Star => {
            let pt = |k: i32| {
                let a = PI / 2.0 + k as f32 * PI / 5.0;
                let r = if k.rem_euclid(2) == 0 { 0.92 } else { 0.38 };
                v(a.cos() * r, a.sin() * r)
            };
            (0..5)
                .map(|k| poly(p, &[Vec2::ZERO, pt(2 * k - 1), pt(2 * k), pt(2 * k + 1)]))
                .fold(f32::MAX, f32::min)
        }
        Icon::Planet => {
            let body = circle(p, Vec2::ZERO, 0.5);
            let q = rot(p, 0.4);
            let ringd = ((q.x / 0.95).powi(2) + (q.y / 0.28).powi(2)).sqrt() - 1.0;
            let ringd = ringd.abs() * 0.3 - 0.03;
            let front = q.y < 0.0 || body > 0.0;
            if front { union(body, ringd) } else { body }
        }
        Icon::Colony => {
            let a = rect(p, v(-0.45, -0.25), v(0.22, 0.5));
            let b = rect(p, v(0.05, -0.05), v(0.22, 0.7));
            let c = rect(p, v(0.52, -0.38), v(0.2, 0.37));
            let base = rect(p, v(0.0, -0.8), v(0.85, 0.06));
            let win = rect(p, v(0.05, 0.25), v(0.08, 0.08));
            sub(union(union(a, b), union(c, base)), win)
        }
        Icon::Rocket => {
            let q = rot(p, -PI / 4.0);
            let body = poly(
                q,
                &[
                    v(0.0, 0.9),
                    v(-0.25, 0.45),
                    v(-0.25, -0.45),
                    v(0.25, -0.45),
                    v(0.25, 0.45),
                ],
            );
            let fin_l = poly(q, &[v(-0.25, -0.1), v(-0.55, -0.6), v(-0.25, -0.45)]);
            let fin_r = poly(q, &[v(0.25, -0.1), v(0.25, -0.45), v(0.55, -0.6)]);
            let flame = poly(q, &[v(-0.15, -0.5), v(0.0, -0.9), v(0.15, -0.5)]);
            let win = circle(q, v(0.0, 0.25), 0.1);
            sub(union(union(body, fin_l), union(fin_r, flame)), win)
        }
        Icon::Flask => {
            let neck = rect(p, v(0.0, 0.55), v(0.14, 0.3));
            let bulb = poly(
                p,
                &[v(-0.14, 0.3), v(-0.65, -0.7), v(0.65, -0.7), v(0.14, 0.3)],
            );
            let rim = rect(p, v(0.0, 0.85), v(0.26, 0.05));
            let outline = union(union(neck, bulb), rim);
            let inner = poly(
                p,
                &[v(-0.42, -0.3), v(-0.55, -0.6), v(0.55, -0.6), v(0.42, -0.3)],
            );
            union(outline.abs() - 0.06, inner)
        }
        Icon::Box | Icon::Cube => {
            let top = poly(
                p,
                &[v(0.0, 0.85), v(-0.75, 0.45), v(0.0, 0.05), v(0.75, 0.45)],
            );
            let left = poly(
                p,
                &[v(-0.75, 0.45), v(-0.75, -0.4), v(0.0, -0.85), v(0.0, 0.05)],
            );
            let right = poly(
                p,
                &[v(0.0, 0.05), v(0.0, -0.85), v(0.75, -0.4), v(0.75, 0.45)],
            );
            let d = union(union(top, left), right);
            // Edges as gaps.
            let edges = union(
                seg(p, v(0.0, 0.05), v(0.0, -0.85), 0.04),
                union(
                    seg(p, v(-0.75, 0.45), v(0.0, 0.05), 0.04),
                    seg(p, v(0.0, 0.05), v(0.75, 0.45), 0.04),
                ),
            );
            sub(d, edges)
        }
        Icon::Gem => {
            let d = poly(
                p,
                &[
                    v(0.0, -0.8),
                    v(0.7, 0.3),
                    v(0.35, 0.7),
                    v(-0.35, 0.7),
                    v(-0.7, 0.3),
                ],
            );
            let facet = union(
                seg(p, v(-0.7, 0.3), v(0.7, 0.3), 0.04),
                union(
                    seg(p, v(-0.3, 0.3), v(0.0, -0.8), 0.035),
                    seg(p, v(0.3, 0.3), v(0.0, -0.8), 0.035),
                ),
            );
            sub(d, facet)
        }
        Icon::Atom => {
            let mut d = circle(p, Vec2::ZERO, 0.16);
            for k in 0..3 {
                let q = rot(p, k as f32 * PI / 3.0);
                let e = ((q.x / 0.9).powi(2) + (q.y / 0.32).powi(2)).sqrt() - 1.0;
                d = union(d, e.abs() * 0.3 - 0.035);
            }
            d
        }
        Icon::Gear => {
            let r = p.length();
            let a = p.y.atan2(p.x);
            let teeth = ((a * 8.0).cos() * 0.5 + 0.5).powf(0.4) * 0.16;
            sub(r - (0.62 + teeth), circle(p, Vec2::ZERO, 0.26))
        }
        Icon::Pick => {
            let handle = seg(p, v(-0.65, -0.75), v(0.25, 0.25), w);
            let head = ring(p, v(-0.35, -0.35), 1.05, 0.09)
                .max(-circle(p, v(-0.35, -0.35), 0.0) - 2.0)
                .max(circle(p, v(0.38, 0.38), 0.62));
            union(handle, head)
        }
        Icon::Flame => {
            let outer = union(
                circle(p, v(0.0, -0.3), 0.5),
                poly(p, &[v(-0.48, -0.2), v(0.48, -0.2), v(0.05, 0.9)]),
            );
            let inner = union(
                circle(p, v(0.0, -0.4), 0.22),
                poly(p, &[v(-0.2, -0.35), v(0.2, -0.35), v(0.0, 0.15)]),
            );
            sub(outer, inner)
        }
        Icon::Factory => {
            let base = rect(p, v(0.0, -0.45), v(0.85, 0.35));
            let saw = poly(p, &[v(-0.85, -0.1), v(-0.3, -0.1), v(-0.85, 0.25)]);
            let saw2 = poly(p, &[v(-0.3, -0.1), v(0.25, -0.1), v(-0.3, 0.25)]);
            let stack = rect(p, v(0.6, 0.25), v(0.13, 0.5));
            let win = union(
                rect(p, v(-0.45, -0.45), v(0.12, 0.1)),
                rect(p, v(0.15, -0.45), v(0.12, 0.1)),
            );
            sub(union(union(base, saw), union(saw2, stack)), win)
        }
        Icon::Bolt => union(
            poly(p, &[v(0.2, 0.95), v(-0.45, -0.1), v(0.1, -0.1)]),
            poly(p, &[v(-0.1, 0.1), v(-0.2, -0.95), v(0.45, 0.1)]),
        ),
        Icon::House => {
            let roof = poly(p, &[v(-0.85, 0.05), v(0.85, 0.05), v(0.0, 0.8)]);
            let body = rect(p, v(0.0, -0.4), v(0.6, 0.45));
            let door = rect(p, v(0.0, -0.6), v(0.15, 0.25));
            sub(union(roof, body), door)
        }
        Icon::Shield => {
            let d = poly(
                p,
                &[
                    v(-0.65, 0.8),
                    v(-0.65, 0.1),
                    v(0.0, -0.9),
                    v(0.65, 0.1),
                    v(0.65, 0.8),
                ],
            );
            let inner = poly(
                p,
                &[v(-0.4, 0.55), v(-0.4, 0.1), v(0.0, -0.55), v(0.0, 0.55)],
            );
            union(sub(d, d + 0.14), inner)
        }
        Icon::Pause => union(
            rect(p, v(-0.28, 0.0), v(0.14, 0.6)),
            rect(p, v(0.28, 0.0), v(0.14, 0.6)),
        ),
        Icon::Play => poly(p, &[v(-0.45, 0.65), v(-0.45, -0.65), v(0.65, 0.0)]),
        Icon::Fast => union(
            poly(p, &[v(-0.8, 0.55), v(-0.8, -0.55), v(0.0, 0.0)]),
            poly(p, &[v(0.0, 0.55), v(0.0, -0.55), v(0.8, 0.0)]),
        ),
        Icon::Person => union(
            circle(p, v(0.0, 0.42), 0.3),
            circle(p, v(0.0, -0.8), 0.62).max(-(p.y + 0.8)),
        ),
        Icon::Warning => {
            let tri = poly(p, &[v(-0.9, -0.75), v(0.9, -0.75), v(0.0, 0.85)]);
            let mark = union(
                rect(p, v(0.0, 0.05), v(0.08, 0.32)),
                circle(p, v(0.0, -0.5), 0.1),
            );
            sub(tri, mark)
        }
        Icon::Belt => {
            let band = rrect(p, Vec2::ZERO, v(0.9, 0.38), 0.38).abs() - 0.07;
            let rollers = union(
                circle(p, v(-0.52, 0.0), 0.16),
                circle(p, v(0.52, 0.0), 0.16),
            );
            union(band, union(rollers, circle(p, Vec2::ZERO, 0.16)))
        }
        Icon::Arm => {
            let a = seg(p, v(-0.6, -0.7), v(-0.2, 0.3), w * 1.2);
            let b = seg(p, v(-0.2, 0.3), v(0.5, 0.45), w * 1.2);
            let base = rect(p, v(-0.6, -0.8), v(0.35, 0.08));
            let claw = union(
                seg(p, v(0.5, 0.45), v(0.75, 0.2), w),
                seg(p, v(0.5, 0.45), v(0.75, 0.7), w),
            );
            union(
                union(a, b),
                union(base, union(claw, circle(p, v(-0.2, 0.3), 0.13))),
            )
        }
        Icon::Trash => {
            let can = poly(
                p,
                &[v(-0.5, 0.45), v(-0.4, -0.85), v(0.4, -0.85), v(0.5, 0.45)],
            );
            let lid = rect(p, v(0.0, 0.6), v(0.65, 0.07));
            let handle = rect(p, v(0.0, 0.75), v(0.18, 0.08));
            let slots = union(
                rect(p, v(-0.18, -0.2), v(0.05, 0.42)),
                rect(p, v(0.18, -0.2), v(0.05, 0.42)),
            );
            sub(union(can, union(lid, handle)), slots)
        }
        Icon::Close => union(
            seg(p, v(-0.6, -0.6), v(0.6, 0.6), w),
            seg(p, v(-0.6, 0.6), v(0.6, -0.6), w),
        ),
        Icon::Chevrons => union(
            chevron(p, -0.35, 0.09),
            union(chevron(p, 0.05, 0.09), chevron(p, 0.45, 0.09)),
        ),
        Icon::Clock => union(
            ring(p, Vec2::ZERO, 0.75, 0.07),
            union(
                seg(p, Vec2::ZERO, v(0.0, 0.5), w),
                seg(p, Vec2::ZERO, v(0.35, -0.15), w),
            ),
        ),
        Icon::Infinity => {
            let a = ring(p, v(-0.38, 0.0), 0.3, 0.08);
            let b = ring(p, v(0.38, 0.0), 0.3, 0.08);
            union(a, b)
        }
        Icon::Target => union(
            ring(p, Vec2::ZERO, 0.7, 0.07),
            union(
                ring(p, Vec2::ZERO, 0.35, 0.07),
                union(
                    circle(p, Vec2::ZERO, 0.1),
                    union(
                        seg(p, v(0.0, 0.55), v(0.0, 0.95), 0.06),
                        seg(p, v(0.55, 0.0), v(0.95, 0.0), 0.06),
                    ),
                ),
            ),
        ),
        Icon::Cursor => union(
            poly(p, &[v(-0.5, 0.85), v(-0.5, -0.55), v(0.5, -0.1)]),
            seg(p, v(-0.1, -0.2), v(0.25, -0.85), 0.1),
        ),
        Icon::Rotate => {
            let arc = ring(p, Vec2::ZERO, 0.6, 0.08).max(-(p.x + 0.0).min(p.y + 0.1));
            let head = poly(p, &[v(0.32, 0.12), v(0.6, -0.3), v(0.88, 0.12)]);
            union(arc, head)
        }
        Icon::Disk => {
            let body = rrect(p, Vec2::ZERO, v(0.75, 0.75), 0.12);
            let label = rect(p, v(0.0, -0.35), v(0.45, 0.3));
            let shutter = rect(p, v(0.1, 0.5), v(0.3, 0.22));
            sub(body, union(label, shutter))
        }
        Icon::Plus => union(
            rect(p, Vec2::ZERO, v(0.6, 0.1)),
            rect(p, Vec2::ZERO, v(0.1, 0.6)),
        ),
        Icon::Minus => rect(p, Vec2::ZERO, v(0.6, 0.1)),
        Icon::Eye => {
            let lens = circle(p, v(0.0, -0.75), 1.05).max(circle(p, v(0.0, 0.75), 1.05));
            sub(lens, circle(p, Vec2::ZERO, 0.32)).min(circle(p, Vec2::ZERO, 0.17))
        }
        Icon::Book => {
            let left = poly(
                p,
                &[
                    v(-0.85, 0.62),
                    v(-0.85, -0.84),
                    v(-0.08, -0.7),
                    v(-0.08, 0.76),
                ],
            );
            let right = poly(
                p,
                &[v(0.08, 0.76), v(0.08, -0.7), v(0.85, -0.84), v(0.85, 0.62)],
            );
            let lines = union(
                union(
                    seg(p, v(-0.66, 0.38), v(-0.24, 0.46), 0.045),
                    seg(p, v(-0.66, 0.12), v(-0.24, 0.2), 0.045),
                ),
                union(
                    union(
                        seg(p, v(-0.66, -0.14), v(-0.24, -0.06), 0.045),
                        seg(p, v(0.24, 0.46), v(0.66, 0.38), 0.045),
                    ),
                    union(
                        seg(p, v(0.24, 0.2), v(0.66, 0.12), 0.045),
                        seg(p, v(0.24, -0.06), v(0.66, -0.14), 0.045),
                    ),
                ),
            );
            sub(union(left, right), lines)
        }
    }
}

fn render(icon: Icon, size: u32) -> Image {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let px = 2.0 / size as f32;
    for y in 0..size {
        for x in 0..size {
            // 3x3 supersampling.
            let mut cover = 0.0;
            for sy in 0..3 {
                for sx in 0..3 {
                    let p = Vec2::new(
                        (x as f32 + (sx as f32 + 0.5) / 3.0) * px - 1.0,
                        1.0 - (y as f32 + (sy as f32 + 0.5) / 3.0) * px,
                    ) * 1.08;
                    if shape(icon, p) <= 0.0 {
                        cover += 1.0;
                    }
                }
            }
            let a = (cover / 9.0 * 255.0) as u8;
            data.extend_from_slice(&[255, 255, 255, a]);
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}
