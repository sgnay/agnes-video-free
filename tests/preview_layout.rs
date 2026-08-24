//! Regression test: the preview image must be fully visible (not vertically
//! clipped) when the source image is taller than the preview box.
//!
//! Mirrors the preview panel element tree from `OcrAppView::render` in
//! `src/app.rs`: a full-height (`flex_1`) overflow-hidden box containing a
//! full-size wrapper div, which contains the `img` with `object-fit: contain`.
//!
//! The bug: when the wrapper div has an auto height, the img's `h_full` (100%)
//! cannot resolve, so the img element grows to its natural height (or its
//! aspect-ratio-derived height) and the overflow-hidden box clips it to the
//! top of the box, hiding the rest of the image. The wrapper must instead
//! resolve to the box's definite height, which in turn constrains the img.

use std::sync::Arc;

use gpui::{div, img, point, prelude::*, px, rgb, size, Bounds, ObjectFit, RenderImage, TestAppContext};
use image::{Frame, ImageBuffer, Rgba};
use smallvec::SmallVec;

fn render_image(width: u32, height: u32) -> Arc<RenderImage> {
    let frame = Frame::new(ImageBuffer::from_pixel(width, height, Rgba([0, 0, 0, 255])));
    Arc::new(RenderImage::new(SmallVec::from_elem(frame, 1)))
}

/// The main content body element tree (preview panel only), copied from
/// `OcrAppView::render`. The right-hand results panel is a grow-2 filler.
fn preview_body(image: Arc<RenderImage>) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .size_full()
        .gap_2()
        .p_2()
        // Preview side panel (1/3 width)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .items_center()
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex_1()
                        .rounded_md()
                        .bg(rgb(0x303030))
                        .border_1()
                        .border_color(rgb(0x505050))
                        .items_center()
                        .justify_center()
                        .overflow_hidden()
                        .debug_selector(|| "img-box".into())
                        .child(
                            div()
                                .w_full()
                                .h_full()
                                .debug_selector(|| "img-wrapper".into())
                                .child(
                                    img(image)
                                        .w_full()
                                        .h_full()
                                        .object_fit(ObjectFit::Contain),
                                ),
                        ),
                ),
        )
        // Results side panel (2/3 width)
        .child(div().flex_1().flex_grow(2.0))
}

#[gpui::test]
fn tall_image_preview_is_not_clipped(cx: &mut TestAppContext) {
    for (w, h, label) in [
        (200u32, 2000u32, "1:10 tall"),
        (4000, 3000, "large landscape"),
        (2000, 8000, "huge tall"),
        (8000, 2000, "huge wide"),
    ] {
        let image = render_image(w, h);
        let window = cx.add_empty_window();
        window.draw(point(px(0.), px(0.)), size(px(1000.), px(700.)), |_, _| {
            preview_body(image).into_any_element()
        });

        let box_bounds = window
            .debug_bounds("img-box")
            .unwrap_or_else(|| panic!("{label}: preview box should be laid out"));
        let wrapper_bounds = window
            .debug_bounds("img-wrapper")
            .unwrap_or_else(|| panic!("{label}: img wrapper should be laid out"));

        assert!(
            box_bounds.size.height > px(0.),
            "{label}: preview box has zero height: {:?}",
            box_bounds
        );

        // The wrapper (and therefore the img inside it, via h_full) must resolve
        // to the box's content height (box height minus the 1px borders). If it
        // resolved to the image's natural height instead, the overflow-hidden
        // box would clip the image.
        let within_box = wrapper_bounds.size.height <= box_bounds.size.height;
        let fills_content = wrapper_bounds.size.height >= box_bounds.size.height - px(4.);
        assert!(
            within_box && fills_content,
            "{label}: img wrapper height {:?} differs from preview box content height (box {:?}): image would be clipped",
            wrapper_bounds.size.height, box_bounds.size.height
        );

        // Neither the preview box nor the wrapper may overflow the window:
        // a large image must never blow the panel up to fullscreen.
        let window_bounds = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1000.), px(700.)),
        };
        assert!(
            box_bounds.origin.x >= window_bounds.origin.x
                && box_bounds.origin.y >= window_bounds.origin.y
                && box_bounds.right() <= window_bounds.right()
                && box_bounds.bottom() <= window_bounds.bottom(),
            "{label}: preview box {:?} overflows window {:?}",
            box_bounds,
            window_bounds
        );
        assert!(
            wrapper_bounds.origin.x >= window_bounds.origin.x
                && wrapper_bounds.origin.y >= window_bounds.origin.y
                && wrapper_bounds.right() <= window_bounds.right()
                && wrapper_bounds.bottom() <= window_bounds.bottom(),
            "{label}: img wrapper {:?} overflows window {:?}",
            wrapper_bounds,
            window_bounds
        );

        // The preview box must stay near its 1:2 share of the width (window
        // 1000px minus padding/gap, roughly 1/3 = ~325px), never expanding to
        // the image's aspect-ratio-derived width.
        assert!(
            box_bounds.size.width < px(400.),
            "{label}: preview box width {:?} expanded far beyond its 1/3 share",
            box_bounds.size.width
        );
    }
}
