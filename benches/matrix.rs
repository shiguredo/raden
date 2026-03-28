use criterion::{Criterion, criterion_group, criterion_main};

use raden::Matrix2D;

const NUM_ITERS: usize = 10_000;

fn bench_matrix(c: &mut Criterion) {
    let mut group = c.benchmark_group("Matrix2D");

    // 後乗算系
    group.bench_function("translate", |b| {
        b.iter(|| {
            let mut m = Matrix2D::IDENTITY;
            for i in 0..NUM_ITERS {
                m.translate(i as f64 * 0.01, i as f64 * 0.02);
            }
            m
        });
    });

    group.bench_function("scale", |b| {
        b.iter(|| {
            let mut m = Matrix2D::IDENTITY;
            for i in 0..NUM_ITERS {
                let s = 1.0 + (i as f64) * 0.0001;
                m.scale(s, s);
            }
            m
        });
    });

    group.bench_function("rotate", |b| {
        b.iter(|| {
            let mut m = Matrix2D::IDENTITY;
            for i in 0..NUM_ITERS {
                m.rotate(i as f64 * 0.001);
            }
            m
        });
    });

    group.bench_function("rotate_around", |b| {
        b.iter(|| {
            let mut m = Matrix2D::IDENTITY;
            for i in 0..NUM_ITERS {
                m.rotate_around(i as f64 * 0.001, 300.0, 400.0);
            }
            m
        });
    });

    // 前乗算系
    group.bench_function("post_translate", |b| {
        b.iter(|| {
            let mut m = Matrix2D::IDENTITY;
            for i in 0..NUM_ITERS {
                m.post_translate(i as f64 * 0.01, i as f64 * 0.02);
            }
            m
        });
    });

    group.bench_function("post_scale", |b| {
        b.iter(|| {
            let mut m = Matrix2D::IDENTITY;
            for i in 0..NUM_ITERS {
                let s = 1.0 + (i as f64) * 0.0001;
                m.post_scale(s, s);
            }
            m
        });
    });

    group.bench_function("post_rotate", |b| {
        b.iter(|| {
            let mut m = Matrix2D::IDENTITY;
            for i in 0..NUM_ITERS {
                m.post_rotate(i as f64 * 0.001);
            }
            m
        });
    });

    group.bench_function("post_transform", |b| {
        let other = Matrix2D::new(0.6, 0.8, -0.8, 0.6, 10.0, 20.0);
        b.iter(|| {
            let mut m = Matrix2D::IDENTITY;
            for _ in 0..NUM_ITERS {
                m.post_transform(&other);
            }
            m
        });
    });

    // multiply
    group.bench_function("multiply", |b| {
        let a = Matrix2D::new(0.6, 0.8, -0.8, 0.6, 10.0, 20.0);
        let other = Matrix2D::new(1.1, 0.1, -0.1, 1.1, 5.0, 3.0);
        b.iter(|| {
            let mut m = a;
            for _ in 0..NUM_ITERS {
                m = m.multiply(&other);
            }
            m
        });
    });

    // map_point
    group.bench_function("map_point", |b| {
        let m = Matrix2D::new(0.6, 0.8, -0.8, 0.6, 100.0, 200.0);
        b.iter(|| {
            let mut sum = 0.0f64;
            for i in 0..NUM_ITERS {
                let (x, y) = m.map_point(i as f64, (NUM_ITERS - i) as f64);
                sum += x + y;
            }
            sum
        });
    });

    group.finish();
}

criterion_group!(benches, bench_matrix);
criterion_main!(benches);
