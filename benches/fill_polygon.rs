use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rand::rngs::ChaCha8Rng;
use rand::{RngExt, SeedableRng};

use raden::{CompOp, Context, FillRule, Image, Path, PipelineRuntime, PixelFormat, Rect, Rgba32};

const CANVAS_WIDTH: u32 = 512;
const CANVAS_HEIGHT: u32 = 600;
const NUM_POLYS: usize = 1000;
const RNG_SEED: u64 = 42;
const RECT_SIZES: [u32; 6] = [8, 16, 32, 64, 128, 256];

struct PolygonData {
    vertices: Vec<(f64, f64)>,
    color: Rgba32,
}

fn gen_polygon_data(num_vertices: usize, size: u32) -> Vec<PolygonData> {
    let mut rng = ChaCha8Rng::seed_from_u64(RNG_SEED);
    let max_x = (CANVAS_WIDTH - size) as f64;
    let max_y = (CANVAS_HEIGHT - size) as f64;
    let wh = size as f64;

    (0..NUM_POLYS)
        .map(|_| {
            let base_x = rng.random_range(0.0..max_x);
            let base_y = rng.random_range(0.0..max_y);
            let vertices: Vec<(f64, f64)> = (0..num_vertices)
                .map(|_| {
                    let x = base_x + rng.random_range(0.0..wh);
                    let y = base_y + rng.random_range(0.0..wh);
                    (x, y)
                })
                .collect();
            let r: u8 = rng.random_range(0..=255);
            let g: u8 = rng.random_range(0..=255);
            let b: u8 = rng.random_range(0..=255);
            let a: u8 = rng.random_range(1..=255);
            PolygonData {
                vertices,
                color: Rgba32::new(r, g, b, a),
            }
        })
        .collect()
}

fn build_polygon_path(vertices: &[(f64, f64)]) -> Path {
    let mut path = Path::new();
    if let Some(&(x, y)) = vertices.first() {
        path.move_to(x, y);
        for &(x, y) in &vertices[1..] {
            path.line_to(x, y);
        }
        path.close();
    }
    path
}

fn warmup(image: &mut Image, runtime: &mut PipelineRuntime) {
    let mut ctx = Context::new(image, runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::rgb(0, 0, 0));
    ctx.fill_all();
    ctx.set_comp_op(CompOp::SrcOver);
    ctx.set_fill_style(Rgba32::new(128, 128, 128, 128));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 8.0, 8.0));
    let mut path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(8.0, 0.0);
    path.line_to(4.0, 8.0);
    path.close();
    ctx.fill_path(&path);
    ctx.set_fill_rule(FillRule::EvenOdd);
    ctx.fill_path(&path);
    ctx.end();
}

fn bench_fill_triangle(c: &mut Criterion) {
    let mut group = c.benchmark_group("FillTriangle");

    let mut image = Image::new(CANVAS_WIDTH, CANVAS_HEIGHT, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    warmup(&mut image, &mut runtime);

    for &size in &RECT_SIZES {
        let polys = gen_polygon_data(3, size);

        group.bench_function(BenchmarkId::new("SrcOver", format!("{size}x{size}")), |b| {
            b.iter(|| {
                let mut ctx = Context::new(&mut image, &mut runtime);
                ctx.set_comp_op(CompOp::SrcOver);
                for poly in &polys {
                    ctx.set_fill_style(poly.color);
                    let path = build_polygon_path(&poly.vertices);
                    ctx.fill_path(&path);
                }
                ctx.end();
            });
        });
    }

    group.finish();
}

fn bench_fill_poly(c: &mut Criterion, num_vertices: usize, fill_rule: FillRule, label: &str) {
    let mut group = c.benchmark_group(label);

    let mut image = Image::new(CANVAS_WIDTH, CANVAS_HEIGHT, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    warmup(&mut image, &mut runtime);

    for &size in &RECT_SIZES {
        let polys = gen_polygon_data(num_vertices, size);

        group.bench_function(BenchmarkId::new("SrcOver", format!("{size}x{size}")), |b| {
            b.iter(|| {
                let mut ctx = Context::new(&mut image, &mut runtime);
                ctx.set_comp_op(CompOp::SrcOver);
                ctx.set_fill_rule(fill_rule);
                for poly in &polys {
                    ctx.set_fill_style(poly.color);
                    let path = build_polygon_path(&poly.vertices);
                    ctx.fill_path(&path);
                }
                ctx.end();
            });
        });
    }

    group.finish();
}

fn bench_fill_poly10_nz(c: &mut Criterion) {
    bench_fill_poly(c, 10, FillRule::NonZero, "FillPoly10NZ");
}

fn bench_fill_poly10_eo(c: &mut Criterion) {
    bench_fill_poly(c, 10, FillRule::EvenOdd, "FillPoly10EO");
}

fn bench_fill_poly20_nz(c: &mut Criterion) {
    bench_fill_poly(c, 20, FillRule::NonZero, "FillPoly20NZ");
}

fn bench_fill_poly20_eo(c: &mut Criterion) {
    bench_fill_poly(c, 20, FillRule::EvenOdd, "FillPoly20EO");
}

fn bench_fill_poly40_nz(c: &mut Criterion) {
    bench_fill_poly(c, 40, FillRule::NonZero, "FillPoly40NZ");
}

fn bench_fill_poly40_eo(c: &mut Criterion) {
    bench_fill_poly(c, 40, FillRule::EvenOdd, "FillPoly40EO");
}

criterion_group!(
    benches,
    bench_fill_triangle,
    bench_fill_poly10_nz,
    bench_fill_poly10_eo,
    bench_fill_poly20_nz,
    bench_fill_poly20_eo,
    bench_fill_poly40_nz,
    bench_fill_poly40_eo,
);
criterion_main!(benches);
