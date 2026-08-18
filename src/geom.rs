//! Diagram-space geometry. Units are SVG user units (px).

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Grow the rectangle by `amount` on every side.
    pub fn inflate(&self, amount: f32) -> Rect {
        Rect::new(
            self.x - amount,
            self.y - amount,
            self.width + amount * 2.0,
            self.height + amount * 2.0,
        )
    }

    pub fn union(&self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    pub fn intersects(&self, other: Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// Where the segment from the rectangle's centre towards `towards` leaves
    /// the rectangle. Used to stop arrows at a box edge instead of its centre.
    pub fn edge_towards(&self, towards: Point) -> Point {
        let center = self.center();
        let dx = towards.x - center.x;
        let dy = towards.y - center.y;
        if dx == 0.0 && dy == 0.0 {
            return center;
        }
        let half_w = self.width / 2.0;
        let half_h = self.height / 2.0;
        // Scale the direction vector until it touches whichever side it hits first.
        let scale_x = if dx == 0.0 {
            f32::INFINITY
        } else {
            half_w / dx.abs()
        };
        let scale_y = if dy == 0.0 {
            f32::INFINITY
        } else {
            half_h / dy.abs()
        };
        let scale = scale_x.min(scale_y);
        Point::new(center.x + dx * scale, center.y + dy * scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_towards_leaves_through_the_nearer_side() {
        let rect = Rect::new(0.0, 0.0, 100.0, 40.0);
        let right = rect.edge_towards(Point::new(500.0, 20.0));
        assert!((right.x - 100.0).abs() < 0.01, "{right:?}");
        assert!((right.y - 20.0).abs() < 0.01, "{right:?}");

        let below = rect.edge_towards(Point::new(50.0, 500.0));
        assert!((below.y - 40.0).abs() < 0.01, "{below:?}");
    }

    #[test]
    fn union_covers_both() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(20.0, 5.0, 10.0, 10.0);
        let u = a.union(b);
        assert_eq!((u.x, u.y, u.right(), u.bottom()), (0.0, 0.0, 30.0, 15.0));
    }

    #[test]
    fn intersects_is_exclusive_at_the_edge() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(a.intersects(Rect::new(5.0, 5.0, 10.0, 10.0)));
        assert!(!a.intersects(Rect::new(10.0, 0.0, 10.0, 10.0)));
    }
}
