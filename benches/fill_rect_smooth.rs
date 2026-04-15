use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rand::rngs::ChaCha8Rng;
use rand::{RngExt, SeedableRng};

use raden::{CompOp, Context, Image, Path, PipelineRuntime, PixelFormat, Rect, Rgba32};

const CANVAS_WIDTH: u32 = 512;
const CANVAS_HEIGHT: u32 = 600;
const NUM_RECTS: usize = 1000;
const RNG_SEED: u64 = 42;
const RECT_SIZES: [u32; 6] = [8, 16, 32, 64, 128, 256];

struct RectData {
    x: f64,
    y: f64,
    color: Rgba32,
}

fn gen_rect_data_smooth(size: u32) -> Vec<RectData> {
    let mut rng = ChaCha8Rng::seed_from_u64(RNG_SEED);
    let max_x = (CANVAS_WIDTH - size) as f64;
    let max_y = (CANVAS_HEIGHT - size) as f64;

    (0..NUM_RECTS)
        .map(|_| {
            let x = rng.random_range(0.0..max_x);
            let y = rng.random_range(0.0..max_y);
            let r: u8 = rng.random_range(0..=255);
            let g: u8 = rng.random_range(0..=255);
            let b: u8 = rng.random_range(0..=255);
            let a: u8 = rng.random_range(1..=255);
            RectData {
                x,
                y,
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
    ctx.fill_rect(&Rect::new(0.5, 0.5, 8.0, 8.0));
    // fill_path のウォームアップ (analytic rasterizer)
    let mut path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(8.0, 0.0);
    path.line_to(8.0, 8.0);
    path.close();
    ctx.fill_path(&path);
    ctx.end();
}

fn bench_fill_rect_smooth(c: &mut Criterion) {
    let mut group = c.benchmark_group("FillRectU");

    let mut image = Image::new(CANVAS_WIDTH, CANVAS_HEIGHT, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    warmup(&mut image, &mut runtime);

    for &size in &RECT_SIZES {
        let rects = gen_rect_data_smooth(size);
        let size_f = size as f64;

        group.bench_function(BenchmarkId::new("SrcOver", format!("{size}x{size}")), |b| {
            b.iter(|| {
                let mut ctx = Context::new(&mut image, &mut runtime);
                ctx.set_comp_op(CompOp::SrcOver);
                for rect_data in &rects {
                    ctx.set_fill_style(rect_data.color);
                    ctx.fill_rect(&Rect::new(rect_data.x, rect_data.y, size_f, size_f));
                }
                ctx.end();
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_fill_rect_smooth);
criterion_main!(benches);
