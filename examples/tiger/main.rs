// Blend2D の tiger デモを raden で再現するサンプル。
// データは AmanithVG (OpenVG 実装) に由来する。

mod tiger_data;

use raden::{
    CompOp, Context, FillRule, Image, Path, PipelineRuntime, PixelFormat, Rgba32, StrokeCap,
    StrokeJoin,
};
use raw_player::{
    Event, KEYCODE_ESCAPE, Renderer, Texture, VideoFormat, Window, init, poll_event, quit,
};

/// パースした 1 つ分のパス情報。
struct TigerPath {
    path: Path,
    fill: Option<(FillRule, Rgba32)>,
    stroke: Option<StrokeStyle>,
}

struct StrokeStyle {
    color: Rgba32,
    width: f64,
    cap: StrokeCap,
    join: StrokeJoin,
    miter_limit: f64,
}

/// tiger データをパースして TigerPath の配列を返す。
fn parse_tiger() -> Vec<TigerPath> {
    let commands = tiger_data::COMMANDS;
    let points = tiger_data::POINTS;
    let h = tiger_data::HEIGHT as f64;

    let mut c = 0usize;
    let mut p = 0usize;
    let mut paths = Vec::new();

    while c < commands.len() {
        // フィルパラメータ
        let fill_rule = match commands[c] {
            b'F' => Some(FillRule::NonZero),
            b'E' => Some(FillRule::EvenOdd),
            _ => None,
        };
        c += 1;

        // ストロークパラメータ
        let has_stroke = commands[c] == b'S';
        c += 1;

        let cap = match commands[c] {
            b'R' => StrokeCap::Round,
            b'S' => StrokeCap::Square,
            _ => StrokeCap::Butt,
        };
        c += 1;

        let join = match commands[c] {
            b'R' => StrokeJoin::Round,
            b'B' => StrokeJoin::Bevel,
            _ => StrokeJoin::MiterBevel,
        };
        c += 1;

        let miter_limit = points[p] as f64;
        let stroke_width = points[p + 1] as f64;
        p += 2;

        // 色 (ストローク RGB, フィル RGB、それぞれ 0.0-1.0)
        let stroke_color = Rgba32::rgb(
            (points[p] * 255.0) as u8,
            (points[p + 1] * 255.0) as u8,
            (points[p + 2] * 255.0) as u8,
        );
        let fill_color = Rgba32::rgb(
            (points[p + 3] * 255.0) as u8,
            (points[p + 4] * 255.0) as u8,
            (points[p + 5] * 255.0) as u8,
        );
        p += 6;

        // パスコマンド
        let count = points[p] as usize;
        p += 1;

        let mut path = Path::new();
        for _ in 0..count {
            match commands[c] {
                b'M' => {
                    path.move_to(points[p] as f64, h - points[p + 1] as f64);
                    p += 2;
                }
                b'L' => {
                    path.line_to(points[p] as f64, h - points[p + 1] as f64);
                    p += 2;
                }
                b'C' => {
                    path.cubic_to(
                        points[p] as f64,
                        h - points[p + 1] as f64,
                        points[p + 2] as f64,
                        h - points[p + 3] as f64,
                        points[p + 4] as f64,
                        h - points[p + 5] as f64,
                    );
                    p += 6;
                }
                b'E' => {
                    path.close();
                }
                _ => {}
            }
            c += 1;
        }

        let fill = fill_rule.map(|rule| (rule, fill_color));
        let stroke = if has_stroke {
            Some(StrokeStyle {
                color: stroke_color,
                width: stroke_width,
                cap,
                join,
                miter_limit,
            })
        } else {
            None
        };

        paths.push(TigerPath { path, fill, stroke });
    }

    paths
}

/// パスの全頂点からバウンディングボックスを計算する。
fn compute_bbox(tiger: &[TigerPath]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for tp in tiger {
        for pt in tp.path.points() {
            min_x = min_x.min(pt.x);
            min_y = min_y.min(pt.y);
            max_x = max_x.max(pt.x);
            max_y = max_y.max(pt.y);
        }
    }
    (min_x, min_y, max_x, max_y)
}

fn render_tiger(
    ctx: &mut Context,
    tiger: &[TigerPath],
    canvas_w: f64,
    canvas_h: f64,
    bbox: (f64, f64, f64, f64),
) {
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::rgb(0, 0, 0x7F));
    ctx.fill_all();

    let (min_x, min_y, max_x, max_y) = bbox;
    let tiger_cx = (min_x + max_x) / 2.0;
    let tiger_cy = (min_y + max_y) / 2.0;
    let s = f64::min(canvas_w / (max_x - min_x), canvas_h / (max_y - min_y));

    ctx.save();
    ctx.translate(-tiger_cx, -tiger_cy);
    ctx.scale(s, s);
    ctx.translate(canvas_w / 2.0, canvas_h / 2.0);

    ctx.set_comp_op(CompOp::SrcOver);

    for tp in tiger {
        if let Some((rule, color)) = tp.fill {
            ctx.set_fill_rule(rule);
            ctx.set_fill_style(color);
            ctx.fill_path(&tp.path);
        }
        if let Some(ref st) = tp.stroke {
            ctx.set_stroke_style(st.color);
            ctx.set_stroke_width(st.width);
            ctx.set_stroke_cap(st.cap);
            ctx.set_stroke_join(st.join);
            ctx.set_stroke_miter_limit(st.miter_limit);
            ctx.stroke_path(&tp.path);
        }
    }

    ctx.restore();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let width: u32 = 580;
    let height: u32 = 520;

    let tiger = parse_tiger();
    let bbox = compute_bbox(&tiger);
    let mut img = Image::new(width, height, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();

    {
        let mut ctx = Context::new(&mut img, &mut runtime);
        render_tiger(&mut ctx, &tiger, width as f64, height as f64, bbox);
        ctx.end();
    }

    init()?;
    let window = Window::new("raden tiger", width as i32, height as i32)?;
    let mut renderer = Renderer::new(&window)?;
    let mut texture = Texture::new(&renderer, VideoFormat::Bgra, width as i32, height as i32)?;

    texture.update_packed(img.data(), img.stride() as i32)?;
    renderer.clear()?;
    renderer.copy(&texture)?;
    renderer.present()?;

    loop {
        while let Some(event) = poll_event() {
            match event {
                Event::Quit | Event::WindowClose => {
                    // SAFETY: quit はメインスレッドから一度だけ呼び出す
                    unsafe { quit() };
                    return Ok(());
                }
                Event::KeyDown { keycode } if keycode == KEYCODE_ESCAPE => {
                    // SAFETY: quit はメインスレッドから一度だけ呼び出す
                    unsafe { quit() };
                    return Ok(());
                }
                _ => {}
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
