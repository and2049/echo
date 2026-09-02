//! The mesh backdrop: a gradient mesh, one node per palette color, each wandering the canvas
//! on its own Lissajous path once per loop. Every pixel blends the nodes by inverse-square
//! distance, with the base pulling like a node a fixed way off, so the colors flow into each
//! other without an edge anywhere and pool back to the base between them. Nothing to blur.

use std::f32::consts::TAU;

use super::palette::{CoverPalette, Rgb};
use super::raster::Raster;
use super::Tone;

const SIZE: (usize, usize) = (128, 80);
/// Per node, prominent color first: the middle of its path, its sweep either way, the turns it
/// makes per loop on each axis (a negative count runs the other way, a 1:2 ratio a figure
/// eight) and where along each it starts, in turns. All as fractions of the canvas.
const NODES: [Node; 4] = [
    Node { center: (0.30, 0.38), sweep: (0.24, 0.22), turns: (1, 1), start: (0.00, 0.25) },
    Node { center: (0.70, 0.32), sweep: (0.22, 0.20), turns: (-1, 1), start: (0.50, 0.00) },
    Node { center: (0.40, 0.68), sweep: (0.28, 0.16), turns: (1, 2), start: (0.25, 0.60) },
    Node { center: (0.72, 0.66), sweep: (0.20, 0.24), turns: (-1, 1), start: (0.75, 0.10) },
];
/// Squared distances start from here, so a node's color plateaus around it instead of spiking.
const SOFTNESS: f32 = 0.02;
/// The base pulls on every pixel like a node this far away.
const AMBIENT_DISTANCE: f32 = 0.45;
const FADE: (f32, f32) = (0.55, 0.95);
const FADE_DEPTH: f32 = 0.6;

struct Node {
    center: (f32, f32),
    sweep: (f32, f32),
    turns: (i32, i32),
    start: (f32, f32),
}

impl Node {
    fn position(&self, phase: f32) -> (f32, f32) {
        let x = self.center.0 + self.sweep.0 * (TAU * (self.turns.0 as f32 * phase + self.start.0)).cos();
        let y = self.center.1 + self.sweep.1 * (TAU * (self.turns.1 as f32 * phase + self.start.1)).sin();
        (x, y)
    }
}

pub fn paint(palette: &CoverPalette, base: Rgb, tone: Tone, phase: f32) -> Raster {
    let (lo, hi) = tone.shape_luminance();
    let nodes: Vec<((f32, f32), Rgb)> = NODES
        .iter()
        .enumerate()
        .map(|(ix, node)| (node.position(phase), palette.color(ix).with_luminance_in(lo, hi)))
        .collect();
    let ambient = 1.0 / (AMBIENT_DISTANCE * AMBIENT_DISTANCE + SOFTNESS);
    let mut raster = Raster::new(SIZE.0, SIZE.1, base);
    raster.map(|x, y, _| {
        let (mut r, mut g, mut b, mut total) = (base.r * ambient, base.g * ambient, base.b * ambient, ambient);
        for ((nx, ny), color) in &nodes {
            let (dx, dy) = (x - nx, y - ny);
            let weight = 1.0 / (dx * dx + dy * dy + SOFTNESS);
            r += color.r * weight;
            g += color.g * weight;
            b += color.b * weight;
            total += weight;
        }
        Rgb::new(r / total, g / total, b / total)
    });
    raster.settle(base, FADE.0, FADE.1, FADE_DEPTH);
    raster
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_close_their_paths_and_roam_the_canvas() {
        for node in &NODES {
            let (x0, y0) = node.position(0.0);
            let (x1, y1) = node.position(1.0);
            assert!((x0 - x1).abs() < 1e-5 && (y0 - y1).abs() < 1e-5);
            let xs: Vec<f32> = (0..32).map(|s| node.position(s as f32 / 32.0).0).collect();
            assert!(xs.iter().cloned().fold(1.0, f32::min) < node.center.0 - 0.15);
            assert!(xs.iter().cloned().fold(0.0, f32::max) > node.center.0 + 0.15);
        }
    }

    #[test]
    fn a_node_colors_the_pixel_under_it() {
        let palette = CoverPalette::from_colors(vec![Rgb::from_u8(30, 60, 230), Rgb::from_u8(230, 40, 40)]);
        let base = Rgb::from_u8(4, 6, 20);
        let raster = paint(&palette, base, Tone::Dark, 0.3);
        let (x, y) = NODES[0].position(0.3);
        let under = raster.get((x * SIZE.0 as f32) as usize, (y * SIZE.1 as f32) as usize);
        assert!(under.b > under.r && under.luminance() > base.luminance() + 0.1);
        let (x, y) = NODES[1].position(0.3);
        let under = raster.get((x * SIZE.0 as f32) as usize, (y * SIZE.1 as f32) as usize);
        assert!(under.r > under.b);
    }
}
