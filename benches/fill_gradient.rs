use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use raden::{Circle, CompOp, Context, Gradient, Image, PipelineRuntime, PixelFormat, Rect, Rgba32};

const CANVAS_WIDTH: u32 = 1920;
const CANVAS_HEIGHT: u32 = 1080;

fn warmup(image: &mut Image, runtime: &mut PipelineRuntime) {
    let mut ctx = Context::new(image, runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::rgb(0, 0, 0));
    ctx.fill_all();
    ctx.set_comp_op(CompOp::SrcOver);
    ctx.set_fill_style(Rgba32::new(128, 128, 128, 128));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 8.0, 8.0));
    ctx.end();
}

fn bench_fill_gradient(c: &mut Criterion) {
    let mut group = c.benchmark_group("FillGradient");

    let mut image = Image::new(CANVAS_WIDTH, CANVAS_HEIGHT, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    warmup(&mut image, &mut runtime);

    let sizes: [(u32, u32); 3] = [(256, 256), (512, 512), (1920, 1080)];

    // --- ベースライン: 単色塗りつぶし ---
    for &(w, h) in &sizes {
        group.bench_function(BenchmarkId::new("Solid/SrcOver", format!("{w}x{h}")), |b| {
            b.iter(|| {
                let mut ctx = Context::new(&mut image, &mut runtime);
                ctx.set_comp_op(CompOp::SrcOver);
                ctx.set_fill_style(Rgba32::new(128, 64, 200, 180));
                ctx.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
                ctx.end();
            });
        });
    }

    // --- Linear Gradient ---
    for &(w, h) in &sizes {
        let mut gradient = Gradient::new_linear(0.0, 0.0, w as f64, h as f64);
        gradient.add_stop(0.0, Rgba32::rgb(255, 0, 0));
        gradient.add_stop(0.5, Rgba32::rgb(0, 255, 0));
        gradient.add_stop(1.0, Rgba32::rgb(0, 0, 255));

        group.bench_function(
            BenchmarkId::new("Linear/SrcOver", format!("{w}x{h}")),
            |b| {
                b.iter(|| {
                    let mut ctx = Context::new(&mut image, &mut runtime);
                    ctx.set_comp_op(CompOp::SrcOver);
                    ctx.set_fill_style_gradient(&gradient);
                    ctx.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
                    ctx.end();
                });
            },
        );
    }

    // --- Radial Gradient ---
    for &(w, h) in &sizes {
        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;
        let r = cx.min(cy);
        let mut gradient = Gradient::new_radial(cx, cy, cx, cy, 0.0, r);
        gradient.add_stop(0.0, Rgba32::rgb(255, 255, 0));
        gradient.add_stop(1.0, Rgba32::rgb(0, 0, 128));

        group.bench_function(
            BenchmarkId::new("Radial/SrcOver", format!("{w}x{h}")),
            |b| {
                b.iter(|| {
                    let mut ctx = Context::new(&mut image, &mut runtime);
                    ctx.set_comp_op(CompOp::SrcOver);
                    ctx.set_fill_style_gradient(&gradient);
                    ctx.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
                    ctx.end();
                });
            },
        );
    }

    // --- Conic Gradient ---
    for &(w, h) in &sizes {
        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;
        let mut gradient = Gradient::new_conic(cx, cy, 0.0);
        gradient.add_stop(0.0, Rgba32::rgb(255, 0, 0));
        gradient.add_stop(0.33, Rgba32::rgb(0, 255, 0));
        gradient.add_stop(0.66, Rgba32::rgb(0, 0, 255));
        gradient.add_stop(1.0, Rgba32::rgb(255, 0, 0));

        group.bench_function(BenchmarkId::new("Conic/SrcOver", format!("{w}x{h}")), |b| {
            b.iter(|| {
                let mut ctx = Context::new(&mut image, &mut runtime);
                ctx.set_comp_op(CompOp::SrcOver);
                ctx.set_fill_style_gradient(&gradient);
                ctx.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
                ctx.end();
            });
        });
    }

    // --- Linear Gradient fill_circle (fill_path 経由) ---
    for &(w, h) in &sizes {
        let r = (w.min(h) as f64) / 2.0;
        let mut gradient = Gradient::new_linear(0.0, 0.0, w as f64, h as f64);
        gradient.add_stop(0.0, Rgba32::rgb(255, 0, 0));
        gradient.add_stop(0.5, Rgba32::rgb(0, 255, 0));
        gradient.add_stop(1.0, Rgba32::rgb(0, 0, 255));

        group.bench_function(BenchmarkId::new("Linear/Circle", format!("{w}x{h}")), |b| {
            b.iter(|| {
                let mut ctx = Context::new(&mut image, &mut runtime);
                ctx.set_comp_op(CompOp::SrcOver);
                ctx.set_fill_style_gradient(&gradient);
                ctx.fill_circle(&Circle::new(w as f64 / 2.0, h as f64 / 2.0, r));
                ctx.end();
            });
        });
    }

    // --- Linear Gradient with fill_alpha = 0.5 (span path フォールバック) ---
    for &(w, h) in &sizes {
        let mut gradient = Gradient::new_linear(0.0, 0.0, w as f64, h as f64);
        gradient.add_stop(0.0, Rgba32::rgb(255, 0, 0));
        gradient.add_stop(0.5, Rgba32::rgb(0, 255, 0));
        gradient.add_stop(1.0, Rgba32::rgb(0, 0, 255));

        group.bench_function(
            BenchmarkId::new("Linear/Alpha05", format!("{w}x{h}")),
            |b| {
                b.iter(|| {
                    let mut ctx = Context::new(&mut image, &mut runtime);
                    ctx.set_comp_op(CompOp::SrcOver);
                    ctx.set_fill_alpha(0.5);
                    ctx.set_fill_style_gradient(&gradient);
                    ctx.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
                    ctx.end();
                });
            },
        );
    }

    // --- Radial Gradient with fill_alpha = 0.5 (JIT row パスをスキップ) ---
    for &(w, h) in &sizes {
        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;
        let r = cx.min(cy);
        let mut gradient = Gradient::new_radial(cx, cy, cx, cy, 0.0, r);
        gradient.add_stop(0.0, Rgba32::rgb(255, 255, 0));
        gradient.add_stop(1.0, Rgba32::rgb(0, 0, 128));

        group.bench_function(
            BenchmarkId::new("Radial/Alpha05", format!("{w}x{h}")),
            |b| {
                b.iter(|| {
                    let mut ctx = Context::new(&mut image, &mut runtime);
                    ctx.set_comp_op(CompOp::SrcOver);
                    ctx.set_fill_alpha(0.5);
                    ctx.set_fill_style_gradient(&gradient);
                    ctx.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
                    ctx.end();
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_fill_gradient);
criterion_main!(benches);
