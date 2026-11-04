//! Operate on widgets that have text.
use crate::Point;
use crate::Vector;
use crate::text::{Fragment, Target};
use crate::widget::operation::{Operation, Outcome};

/// The internal state of a widget that has text.
pub trait Text {
    /// Returns the text of the widget.
    fn text(&self) -> Fragment<'_>;

    /// Selects text between the given positions in layout coordinates.
    ///
    /// `start` is the position where the user started selecting from,
    /// while `end` is the final position of the selection.
    fn select(&mut self, start: Point, end: Point, target: Target);

    /// Selects all the text of the widget.
    fn select_all(&mut self);

    /// Deselects any selected text;
    fn deselect(&mut self);

    /// Returns the selected text, if any.
    ///
    // TODO: Make immutable
    fn copy(&mut self) -> Option<String>;
}

impl Text for String {
    fn text(&self) -> Fragment<'_> {
        self.as_str().into()
    }

    fn select(&mut self, _start: Point, _end: Point, _target: Target) {}
    fn select_all(&mut self) {}
    fn deselect(&mut self) {}

    fn copy(&mut self) -> Option<String> {
        None
    }
}

/// Selects text between the given positions in layout coordinates.
///
/// `start` is the position where the user started selecting from,
/// while `end` is the final position of the selection.
pub fn select(start: Point, end: Point, target: Target) -> impl Operation {
    struct Select {
        start: Point,
        end: Point,
        target: Target,
        translation: Vector,
        in_scope: bool,
    }

    impl Operation for Select {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            let translation = self.translation;
            let in_scope = self.in_scope;
            operate(self);
            self.translation = translation;
            self.in_scope = in_scope;
        }

        fn text_scope(&mut self, bounds: crate::Rectangle) {
            self.in_scope &= bounds.contains(self.start + self.translation);
        }

        fn scrollable(
            &mut self,
            _id: Option<&crate::widget::Id>,
            _bounds: crate::Rectangle,
            _content: crate::Size,
            translation: crate::Vector,
            _state: &mut dyn super::Scrollable,
        ) {
            self.translation += translation;
        }

        fn text(
            &mut self,
            _id: Option<&crate::widget::Id>,
            _bounds: crate::Rectangle,
            state: &mut dyn Text,
        ) {
            if self.in_scope {
                state.select(
                    self.start + self.translation,
                    self.end + self.translation,
                    self.target,
                );
            } else {
                state.deselect();
            }
        }
    }

    Select {
        start,
        end,
        target,
        translation: Vector::ZERO,
        in_scope: true,
    }
}

/// Selects all selectable text.
pub fn select_all() -> impl Operation {
    struct SelectAll;

    impl Operation for SelectAll {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn text(
            &mut self,
            _id: Option<&crate::widget::Id>,
            _bounds: crate::Rectangle,
            state: &mut dyn Text,
        ) {
            state.select_all();
        }
    }

    SelectAll
}

/// Deselects any selected text.
pub fn deselect() -> impl Operation {
    struct Deselect;

    impl Operation for Deselect {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn text(
            &mut self,
            _id: Option<&crate::widget::Id>,
            _bounds: crate::Rectangle,
            state: &mut dyn Text,
        ) {
            state.deselect();
        }
    }

    Deselect
}

/// Collects all the selected text.
pub fn copy() -> impl Operation<String> {
    struct Copy {
        text: String,
    }

    impl Operation<String> for Copy {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<String>)) {
            operate(self);
        }

        fn text(
            &mut self,
            _id: Option<&crate::widget::Id>,
            _bounds: crate::Rectangle,
            state: &mut dyn Text,
        ) {
            let Some(text) = state.copy() else {
                return;
            };

            if text.is_empty() {
                return;
            }

            if !self.text.is_empty() {
                self.text.push('\n');
            }

            self.text.push_str(&text);
        }

        fn finish(&self) -> Outcome<String> {
            Outcome::Some(self.text.clone())
        }
    }

    Copy {
        text: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rectangle;
    use crate::widget::operation;

    #[derive(Default)]
    struct TextState(Option<(Point, Point)>);

    impl Text for TextState {
        fn text(&self) -> Fragment<'_> {
            "text".into()
        }
        fn select(&mut self, start: Point, end: Point, _: Target) {
            self.0 = Some((start, end));
        }
        fn select_all(&mut self) {
            self.0 = Some((Point::ORIGIN, Point::ORIGIN));
        }
        fn deselect(&mut self) {
            self.0 = None;
        }
        fn copy(&mut self) -> Option<String> {
            self.0.map(|_| "text".into())
        }
    }

    fn region(operation: &mut dyn Operation, x: f32, text: &mut TextState) {
        operation.traverse(&mut |operation| {
            let bounds = Rectangle {
                x,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            };
            operation.text_scope(bounds);
            operation.text(None, bounds, text);
        });
    }

    #[test]
    fn selection_stays_in_origin_region_and_clears_other_regions_through_adapters() {
        let start = Point::new(150.0, 10.0);
        // The endpoint can leave the origin region while dragging.
        let end = Point::new(10.0, 80.0);
        let operations: Vec<Box<dyn Operation>> = vec![
            Box::new(select(start, end, Target::Character)),
            Box::new(operation::map(select(start, end, Target::Word), |()| ())),
            Box::new(operation::then(select(start, end, Target::Line), |()| {
                deselect()
            })),
        ];
        for mut operation in operations {
            let mut left = TextState::default();
            let mut right = TextState::default();
            left.select_all();
            let mut erased = operation::black_box::<(), ()>(&mut operation);
            region(&mut erased, 0.0, &mut left);
            region(&mut erased, 100.0, &mut right);
            assert!(left.0.is_none());
            assert_eq!(right.0, Some((start, end)));
        }
    }

    #[test]
    fn nested_regions_cannot_reenable_selection_outside_an_ancestor() {
        let start = Point::new(150.0, 10.0);
        let end = Point::new(150.0, 80.0);
        let mut operation = select(start, end, Target::Character);
        let mut rejected = TextState::default();
        rejected.select_all();
        operation.traverse(&mut |operation| {
            operation.text_scope(Rectangle {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            });
            region(operation, 100.0, &mut rejected);
        });
        assert!(rejected.0.is_none());
        let mut accepted = TextState::default();
        region(&mut operation, 100.0, &mut accepted);
        assert_eq!(accepted.0, Some((start, end)));
    }

    #[test]
    fn explicit_select_all_and_copy_still_visit_every_region() {
        let mut left = TextState::default();
        let mut right = TextState::default();
        let mut all = select_all();
        region(&mut all, 0.0, &mut left);
        region(&mut all, 100.0, &mut right);
        let mut copy = copy();
        {
            let mut erased = operation::black_box(&mut copy);
            region(&mut erased, 0.0, &mut left);
            region(&mut erased, 100.0, &mut right);
        }
        assert!(matches!(copy.finish(), Outcome::Some(text) if text == "text\ntext"));
    }
}
