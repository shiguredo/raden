#![allow(clippy::too_many_arguments, clippy::type_complexity)]

pub mod api;
pub mod codec;
pub mod font;
pub mod pipeline;
pub mod pixel;
pub mod raster;

pub use api::context::{Arc, Circle, Context, Line, Rect};
pub use api::image::Image;
pub use api::matrix::Matrix2D;
pub use api::path::{Path, PathCmd, Point};
pub use api::style::{CompOp, FillRule, Rgba32, StrokeCap, StrokeJoin};
pub use font::{Font, FontData, FontError, FontFace};
pub use pipeline::runtime::PipelineRuntime;
pub use pixel::PixelFormat;
pub use pixel::premultiply_rgba;
