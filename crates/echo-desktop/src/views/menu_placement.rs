//! Measure and fit popups before painting, in window coordinates.
//!
//! GPUI's anchored element only applies its inset when a panel crosses the raw viewport
//! edge. Fit against the visible client area instead, including when a panel would only
//! overlap the transparent frame. Measuring in prepaint avoids height estimates and jitter.
use std::{cell::Cell, rc::Rc};

use gpui::{
    AnyElement, App, Bounds, Display, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Point, Position, Size, Style, Tiling, Window, point, px,
};

pub(super) enum MenuAnchor {
    Pointer(Point<Pixels>),
    Center,
    Row(Rc<Cell<Bounds<Pixels>>>),
}

pub(super) fn menu_bounds(window: &Window) -> Bounds<Pixels> {
    safe_bounds(window.viewport_size(), super::client_corners(window))
}

fn safe_bounds(size: Size<Pixels>, tiling: Option<Tiling>) -> Bounds<Pixels> {
    // Eight pixels also keeps rectangular panels out of the ten-pixel corner arcs.
    let inset = |tiled: bool| {
        px(8.0
            + if tiling.is_some() && !tiled {
                super::CLIENT_SHADOW
            } else {
                0.0
            })
    };
    let tiled = tiling.unwrap_or_default();
    let origin = point(inset(tiled.left), inset(tiled.top));
    Bounds::new(
        origin,
        gpui::size(
            (size.width - origin.x - inset(tiled.right)).max(px(0.0)),
            (size.height - origin.y - inset(tiled.bottom)).max(px(0.0)),
        ),
    )
}

fn fit(origin: Point<Pixels>, size: Size<Pixels>, limits: Bounds<Pixels>) -> Point<Pixels> {
    point(
        origin.x.min(limits.right() - size.width).max(limits.left()),
        origin
            .y
            .min(limits.bottom() - size.height)
            .max(limits.top()),
    )
}

pub(super) fn menu_placement(anchor: MenuAnchor, child: impl IntoElement) -> MenuPlacement {
    MenuPlacement {
        anchor,
        child: child.into_any_element(),
    }
}

pub(super) struct MenuPlacement {
    anchor: MenuAnchor,
    child: AnyElement,
}

impl IntoElement for MenuPlacement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for MenuPlacement {
    type RequestLayoutState = LayoutId;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, LayoutId) {
        let child = self.child.request_layout(window, cx);
        let layout = window.request_layout(
            Style {
                position: Position::Absolute,
                display: Display::Flex,
                ..Style::default()
            },
            [child],
            cx,
        );
        (layout, child)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut LayoutId,
        window: &mut Window,
        cx: &mut App,
    ) {
        let size = window.layout_bounds(*child).size;
        let limits = menu_bounds(window);
        let origin = match &self.anchor {
            MenuAnchor::Pointer(position) => *position,
            MenuAnchor::Center => {
                limits.origin
                    + point(
                        (limits.size.width - size.width) / 2.0,
                        (limits.size.height - size.height) / 2.0,
                    )
            }
            MenuAnchor::Row(row) => {
                // The parent menu prepaints first, so this includes its final placement.
                let row = row.get();
                let left = if row.right() + size.width > limits.right() {
                    row.left() - size.width
                } else {
                    row.right()
                };
                point(left, row.top() - px(5.0))
            }
        };
        let offset = fit(origin, size, limits) - bounds.origin;
        window.with_element_offset(offset, |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut LayoutId,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    #[test]
    fn fits_edges_without_moving_interior_clicks() {
        let bounds = safe_bounds(size(px(640.0), px(480.0)), None);
        let panel = size(px(210.0), px(170.0));
        assert_eq!(
            fit(point(px(100.0), px(100.0)), panel, bounds),
            point(px(100.0), px(100.0))
        );
        assert_eq!(
            fit(point(px(630.0), px(470.0)), panel, bounds),
            point(px(422.0), px(302.0))
        );
        // Still inside the raw viewport, but overlapping the safe gutter.
        assert_eq!(
            fit(point(px(428.0), px(308.0)), panel, bounds),
            point(px(422.0), px(302.0))
        );
        assert_eq!(
            fit(point(px(-5.0), px(-5.0)), panel, bounds),
            point(px(8.0), px(8.0))
        );
    }

    #[test]
    fn excludes_only_untiled_client_frame_edges() {
        let bounds = safe_bounds(
            size(px(640.0), px(480.0)),
            Some(Tiling {
                left: true,
                top: true,
                ..Tiling::default()
            }),
        );
        assert_eq!(bounds.origin, point(px(8.0), px(8.0)));
        assert_eq!(bounds.bottom_right(), point(px(622.0), px(462.0)));
    }
}
