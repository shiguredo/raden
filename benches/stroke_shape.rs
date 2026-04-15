#[path = "shape_data.rs"]
mod shape_data;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rand::rngs::ChaCha8Rng;
use rand::{RngExt, SeedableRng};

use raden::{CompOp, Context, Image, Path, PipelineRuntime, PixelFormat, Rect, Rgba32};
use shape_data::{ShapeKind, build_path};

const CANVAS_WIDTH: u32 = 512;
const CANVAS_HEIGHT: u32 = 600;
const NUM_SHAPES: usize = 1000;
const RNG_SEED: u64 = 42;
const RECT_SIZES: [u32; 6] = [8, 16, 32, 64, 128, 256];
const STROKE_WIDTH: f64 = 2.0;

struct ShapePlacement {
    tx: f64,
    ty: f64,
    color: Rgba32,
}

fn gen_shape_placements(size: u32) -> Vec<ShapePlacement> {
    let mut rng = ChaCha8Rng::seed_from_u64(RNG_SEED);
    let max_x = (CANVAS_WIDTH - size) as f64;
    let max_y = (CANVAS_HEIGHT - size) as f64;

    (0..NUM_SHAPES)
        .map(|_| {
            let tx = rng.random_range(0.0..max_x);
            let ty = rng.random_range(0.0..max_y);
            let r: u8 = rng.random_range(0..=255);
            let g: u8 = rng.random_range(0..=255);
            let b: u8 = rng.random_range(0..=255);
            let a: u8 = rng.random_range(1..=255);
            ShapePlacement {
                tx,
                ty,
                color: Rgba32::new(r, g, b, a),
            }
        })
        .collect()
}

fn warmup(image: &mut Image, runtime: &mut PipelineRuntime) {
    let mut ctx = Context::new(image, runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::rgb(0, 0, 0));
    ctx.fill_all();
    ctx.set_comp_op(CompOp::SrcOver);
    ctx.set_fill_style(Rgba32::new(128, 128, 128, 128));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 8.0, 8.0));
    ctx.set_stroke_width(STROKE_WIDTH);
    ctx.set_stroke_style(Rgba32::new(128, 128, 128, 128));
    let mut path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(8.0, 0.0);
    path.line_to(4.0, 8.0);
    path.close();
    ctx.stroke_path(&path);
    ctx.end();
}

fn bench_stroke_shape_kind(c: &mut Criterion, kind: ShapeKind) {
    let mut group = c.benchmark_group(format!("Stroke{}", kind.name()));

    let mut image = Image::new(CANVAS_WIDTH, CANVAS_HEIGHT, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    warmup(&mut image, &mut runtime);

    for &size in &RECT_SIZES {
        let placements = gen_shape_placements(size);
        let shape_path = build_path(kind, size as f64);

        group.bench_function(BenchmarkId::new("SrcOver", format!("{size}x{size}")), |b| {
            b.iter(|| {
                let mut ctx = Context::new(&mut image, &mut runtime);
                ctx.set_comp_op(CompOp::SrcOver);
                ctx.set_stroke_width(STROKE_WIDTH);
                for placement in &placements {
                    ctx.set_stroke_style(placement.color);
                    ctx.save();
                    ctx.translate(placement.tx, placement.ty);
                    ctx.stroke_path(&shape_path);
                    ctx.restore();
                }
                ctx.end();
            });
        });
    }

    group.finish();
}

fn bench_stroke_butterfly(c: &mut Criterion) {
    bench_stroke_shape_kind(c, ShapeKind::Butterfly);
}

fn bench_stroke_fish(c: &mut Criterion) {
    bench_stroke_shape_kind(c, ShapeKind::Fish);
}

fn bench_stroke_dragon(c: &mut Criterion) {
    bench_stroke_shape_kind(c, ShapeKind::Dragon);
}

fn bench_stroke_world(c: &mut Criterion) {
    bench_stroke_shape_kind(c, ShapeKind::World);
}

criterion_group!(
    benches,
    bench_stroke_butterfly,
    bench_stroke_fish,
    bench_stroke_dragon,
    bench_stroke_world,
);
criterion_main!(benches);
